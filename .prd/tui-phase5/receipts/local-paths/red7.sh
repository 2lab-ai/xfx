#!/usr/bin/env bash
# LOCAL-PATHS round 7: each finding of the review of round 6, shown on a
# tree's own code (a throwaway clone at a revision, or the worktree), one line
# of verdict each. Paths it prints are written as placeholders or stand-ins.
#   red7.sh <tree>
set -u
tree=$1
work=$(mktemp -d "${TMPDIR:-/tmp}/lp-red7.XXXXXX")
trap 'rm -rf "$work"' EXIT
say() { printf '%-48s %s\n' "$1" "$2"; }

# 1. capture-tmpdir: `fxtmp` before anything but `/`. 2. A root ending in a
# gutter run, whole inside a row (conservative: refused).
python3 -I - "$tree" <<'EOF'
import os, sys
sys.dont_write_bytecode = True
tree = sys.argv[1]
fx = os.path.join(tree, "scripts", "fxcap")
sys.path.insert(0, fx)
import roots, cells, ledger
def say(a, b):
    print("%-48s %s" % (a, b))
led = ledger.load(os.path.join(fx, "parity-ledger.toml"), tree)
def same(a, b):
    a = cells.normalize(cells.parse(a), ledger.rules_for(led, "fx"))
    b = cells.normalize(cells.parse(b), ledger.rules_for(led, "xfx"))
    return not cells.differing(a, b)
run = "<scratch______________>"
for name in ("fxtmp,backup/a", "fxtmp backup/a", "fxtmp:x", "fxtmp)", "fxtmp", "fxtmp\"", "fxtmp|", "fxtmp-old/a", "fxtmpé/a"):
    say("1 <scratch>/%s:" % name.replace("\t", "\\t"), "normalized (RED)" if same("a %s/%s" % (run, name), "a %s/x%s" % (run, name)) else "compared")
say("1 <scratch>/fxtmp/b.md:", "normalized" if same("a %s/fxtmp/b.md" % run, "a %s/xfxtmp/b.md" % run) else "compared (RED)")
rules = roots.rules_for([("/tmp/fxt ", "tmp")])
for data in (b"x /tmp/fxt /notes\n", b"x /tmp/fxt y\n"):
    try:
        roots.rewrite(data, rules)
        how = "written"
    except roots.Refused:
        how = "refused (conservative)"
    say("2 `/tmp/fxt ` whole in %r:" % data.decode().strip(), how)
EOF

# 3. capture.sh's `dir` and a binary's TMPDIR name: refused before launch?
d=$work/dir
mkdir -p "$d/scen"
printf 'dir fxtmp-old\nwait 1\nsnap a\n' >"$d/scen/t.cap"
mkdir -p "$d/bin"
ln -s /bin/bash "$d/bin/it"
status=0
env -u TMUX -u TMUX_PANE FXCAP_SCENARIOS="$d/scen" TMPDIR="$d" FX_BIN="$d/bin/it" \
	bash "$tree/scripts/fxcap/capture.sh" fx t 30x200 "$d/out" >"$work/cap.log" 2>&1 </dev/null || status=$?
for sock in "$d"/fxcap.*/s; do [ -S "$sock" ] && tmux -f /dev/null -S "$sock" kill-server 2>/dev/null; done
if [ "$status" -eq 2 ] && grep -q 'dir takes a plain name' "$work/cap.log"; then
	say "3 dir fxtmp-old:" "refused, exit 2"
else
	say "3 dir fxtmp-old:" "made beside the TMPDIRs, exit $status (RED)"
fi

# 4. The check: a scratch root broken after its tmp/, led by a backtick, and
# a model path that only looks like one.
t=$work/check
mkdir -p "$t/scripts"
cp "$tree/scripts/check-no-local-paths.sh" "$t/scripts/"
s=/
printf 'in the harness temp folder (`claude-%s%sp5uc`)\n' 501 "$s" >"$t/ticked.md"
printf 'models%sclaude-3%sx anthropic.claude-3%sx\n' "$s" "$s" "$s" >"$t/model.md"
GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$t" init -q
GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$t" add -A -f
out=$(cd "$t" && ./scripts/check-no-local-paths.sh 2>&1 </dev/null) || true
case "$out" in *"ticked.md:1:"*) say "4 (\`claude-<digits>/p5uc\`):" "refused" ;; *) say "4 (\`claude-<digits>/p5uc\`):" "passes (RED)" ;; esac
case "$out" in *"model.md:1:"*) say "4 models/claude-3/x:" "refused (RED)" ;; *) say "4 models/claude-3/x:" "passes" ;; esac
