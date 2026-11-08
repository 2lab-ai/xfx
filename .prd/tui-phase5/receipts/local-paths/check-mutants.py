"""LOCAL-PATHS: each mutant of scripts/check-no-local-paths.sh, applied to a
throwaway clone's copy of it (never a checkout anyone works in), must be
failed by scripts/test-check-no-local-paths.sh.

usage: python3 -I check-mutants.py <clone>
"""
import os
import subprocess
import sys

clone = sys.argv[1]
p = os.path.join(clone, "scripts", "check-no-local-paths.sh")
orig = open(p).read()
M = [
    ("no placeholder list", "\t\t\twhile [[ $bare =~ $allowed_re ]]; do", "\t\t\twhile false; do"),
    ("no placeholder-prefix strip", '\t\t\tfor entry in "${placeholders[@]}"; do', "\t\t\tfor entry in; do"),
    ("no mark", "\t\t\t\tmarked=$((marked + 1))\n\t\t\t\tcontinue", "\t\t\t\t:"),
    ("a mark anywhere", '\t\t\tif [ "$markable" = yes ] && [[ $text =~ $marked_re ]]; then', '\t\t\tif [[ $text == *"$marker"* ]]; then'),
    ("a mark in a capture", "is_capture() {\n", "is_capture() {\n\treturn 1\n"),
    ("no start anchor", 'start="(^|[^A-Za-z0-9._~+-]|${control})"', "start=''"),
    ("no SGR start", 'start="(^|[^A-Za-z0-9._~+-]|${control})"', "start='(^|[^A-Za-z0-9._~+-])'"),
    ("a > starts no path", 'start="(^|[^A-Za-z0-9._~+-]|${control})"', 'start="(^|[^A-Za-z0-9._~+>-]|${control})"'),
    ("a home needs its slash", "end='([^A-Za-z0-9._-]|$)'", "end='/'"),
    ("no GIT_* drop", '\t*) unset "$name" ;;', "\t*) : ;;"),
    ("no binary probe", '\t\tif [ "$probe" -ne 0 ]; then', "\t\tif false; then"),
    ("no index-bit refusal", '\tS\\ * | [[:lower:]]\\ *) flagged+=("${entry#??}") ;;', "\tS\\ * | [[:lower:]]\\ *) ;;"),
    ("no missing-file refusal", 'if [ -s "$work/deleted" ]; then', "if false; then"),
    ("no root shape", '\t"a harness scratch root:/tmp/claud[e]-[0-9]+"\n', ""),
    ("no broken-root shape", '\t"a harness scratch root, broken after its tmp/:(^|[[:space:]]|${control})claud[e]-[0-9]+/"\n', ""),
]
env = dict(os.environ)
for k in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
    env.pop(k, None)


def run():
    return subprocess.run(["bash", os.path.join(clone, "scripts", "test-check-no-local-paths.sh")], capture_output=True,
                          text=True, env=env, cwd=clone, stdin=subprocess.DEVNULL, timeout=300)


for name, old, new in M:
    assert orig.count(old) == 1, (name, orig.count(old))
    open(p, "w").write(orig.replace(old, new))
    r = run()
    fails = [line.split("FAIL ", 1)[1].split(":")[0] for line in r.stderr.splitlines() if "FAIL " in line]
    print("%-28s exit %d  failed: %s" % (name, r.returncode, ", ".join(fails) or "none"))
open(p, "w").write(orig)
r = run()
print("%-28s exit %d  %s" % ("unmutated", r.returncode, r.stdout.strip().splitlines()[-1]))
