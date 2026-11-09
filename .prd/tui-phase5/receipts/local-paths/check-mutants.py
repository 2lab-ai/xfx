"""LOCAL-PATHS: each mutant of scripts/check-no-local-paths.sh, applied to a
throwaway clone's copy of it (never a checkout anyone works in), must be
failed by scripts/test-check-no-local-paths.sh.

Round 4: each mutant runs twice. As it is, the check's own positive control
(its planted tree) refuses most of them before the scan runs, and every case
of the test fails alike. Then with that control's verdict taken out as well,
so the line says which of the test's own cases and plants fail it without
the control's help.

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
    ("no placeholder-prefix strip", "\t\t\twhile [[ $bare =~ $placeholder_home_re ]]; do", "\t\t\twhile false; do"),
    ("no sentinel", '\t\t\tbare="$text "', '\t\t\tbare="$text"'),
    ("a report is no capture", "\tif [[ $1 =~ $capture_name_re ]]; then", "\tif false; then"),
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
    ("no broken-root shape", '\t"a harness scratch root, broken after its tmp/:(^|[^A-Za-z0-9._~+/-]|${control})claud[e]-[0-9]+/"\n', ""),
    ("broken root after a space only (round 6)", '(^|[^A-Za-z0-9._~+/-]|${control})claud[e]', '(^|[[:space:]]|${control})claud[e]'),
    ("broken root after a slash too", '(^|[^A-Za-z0-9._~+/-]|${control})claud[e]', '(^|[^A-Za-z0-9._~+-]|${control})claud[e]'),
]
# The positive control's two verdicts, made to pass whatever it found.
NO_CONTROL = [
    ('elif [ "$(sort "$work/got")" != "$(printf \'%s\\n\' "$want" | sort)" ]; then', "elif false; then"),
    ('elif [ "$marked" -ne 1 ] || [ "$placeheld" -ne 2 ]; then', "elif false; then"),
]
env = dict(os.environ)
for k in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
    env.pop(k, None)


def run():
    return subprocess.run(["bash", os.path.join(clone, "scripts", "test-check-no-local-paths.sh")], capture_output=True,
                          text=True, env=env, cwd=clone, stdin=subprocess.DEVNULL, timeout=300)


def failed(r):
    return [line.split("FAIL ", 1)[1].split(":")[0] for line in r.stderr.splitlines() if "FAIL " in line]


uncontrolled = orig
for old, new in NO_CONTROL:
    assert uncontrolled.count(old) == 1, old
    uncontrolled = uncontrolled.replace(old, new)
for name, old, new in M:
    assert orig.count(old) == 1, (name, orig.count(old))
    open(p, "w").write(orig.replace(old, new))
    r = run()
    print("%-28s exit %d  failed: %s" % (name, r.returncode, ", ".join(failed(r)) or "none"))
    open(p, "w").write(uncontrolled.replace(old, new))
    r = run()
    print("%-28s exit %d  failed: %s" % ("  without the control", r.returncode, ", ".join(failed(r)) or "none"))
open(p, "w").write(orig)
r = run()
print("%-28s exit %d  %s" % ("unmutated", r.returncode, r.stdout.strip().splitlines()[-1]))
open(p, "w").write(uncontrolled)
r = run()
print("%-28s exit %d  %s" % ("unmutated, no control", r.returncode, r.stdout.strip().splitlines()[-1]))
open(p, "w").write(orig)
