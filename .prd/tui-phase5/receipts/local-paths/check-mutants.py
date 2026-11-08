import os, subprocess, sys
clone = sys.argv[1]
p = os.path.join(clone, "scripts", "check-no-local-paths.sh")
orig = open(p).read()
M = [
 ("no placeholder list", '\t\tbare="${bare//"$entry"/}"', '\t\t:'),
 ("no mark", "\t\t\t\tmarked=$((marked + 1))\n\t\t\t\tcontinue", "\t\t\t\t:"),
 ("no start anchor", "start=\"(^|[^A-Za-z0-9._~+>-]|${esc}\\\\[[0-9;:?]*[@-~])\"", "start=''"),
 ("no SGR start", "start=\"(^|[^A-Za-z0-9._~+>-]|${esc}\\\\[[0-9;:?]*[@-~])\"", "start='(^|[^A-Za-z0-9._~+>-])'"),
 ("no GIT_* drop", '\t*) unset "$name" ;;', '\t*) : ;;'),
 ("no binary probe", '\t\tif [ "$probe" -ne 0 ]; then', '\t\tif false; then'),
 ("no index-bit refusal", '\tS\\ * | [[:lower:]]\\ *) flagged+=("${entry#??}") ;;', '\tS\\ * | [[:lower:]]\\ *) ;;'),
 ("no missing-file refusal", 'if [ -s "$work/deleted" ]; then', 'if false; then'),
 ("no root shape", '\t"a harness scratch root:/tmp/claud[e]-[0-9]+"\n', ''),
]
env = dict(os.environ)
for k in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
    env.pop(k, None)
for name, old, new in M:
    assert orig.count(old) == 1, (name, orig.count(old))
    open(p, "w").write(orig.replace(old, new))
    r = subprocess.run(["bash", os.path.join(clone, "scripts", "test-check-no-local-paths.sh")], capture_output=True, text=True, env=env, cwd=clone, stdin=subprocess.DEVNULL, timeout=300)
    fails = [l.split("FAIL ", 1)[1].split(":")[0] for l in r.stderr.splitlines() if "FAIL " in l]
    print("%-24s exit %d  failed: %s" % (name, r.returncode, ", ".join(fails) or "none"))
open(p, "w").write(orig)
r = subprocess.run(["bash", os.path.join(clone, "scripts", "test-check-no-local-paths.sh")], capture_output=True, text=True, env=env, cwd=clone, stdin=subprocess.DEVNULL, timeout=300)
print("%-24s exit %d  %s" % ("unmutated", r.returncode, r.stdout.strip().splitlines()[-1]))
