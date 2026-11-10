#!/usr/bin/env bash
# LOCAL-PATHS-WRAP: roots.py --check and scripts/check-no-local-paths.sh over
# every tracked file of <tree> -- each committed capture and receipt among
# them -- with <tree>'s roots.py, or the one given. The roots are built here at
# run time, never written in this file, in two sets:
#   machine  this machine's HOME and harness scratch root (/tmp/claude-<uid>,
#            and the resolved form roots.py adds), the scenario directories
#            of <tree>, of the checkout whose .git it shares and of a
#            GitHub-hosted runner's checkout (a tail of each is the same), and
#            a run's scratch and work roots with a made-up name (their tails
#            but the random name's are every run's; flake.py reads those)
#   public   a runner's HOME and bun, and the bun on PATH when it is outside
#            HOME: paths that name no user, which capture.sh writes as
#            placeholders in a run of its own
# Over the rest, a hit is a source line marked `local-path-ok` or a public
# path; scan.txt counts which. It prints what each check said -- a tracked file's name and a root's
# placeholder name, never a root -- and how many rows of tracked files open
# behind a mark (roots.MARKS), which the tail rule reads.
#   scan.sh <tree> [<roots.py>]
set -u
set -o pipefail
tree=$1
cd "$tree" || exit 2
roots_py=${2:-$tree/scripts/fxcap/roots.py}
common=$(cd "$(git rev-parse --git-common-dir)/.." && pwd)
# A runner's paths assembled, so that this tracked file holds none of them.
rn=runner
runner_home=/home/$rn
# A run directory's name made up, likewise assembled.
made=/x/fxcap
made=$made.ABCDEF
machine=(--root "$HOME" home --root "/tmp/claude-$(id -u)" tmp
  --root "$tree/scripts/fxcap/scenarios" scenarios --root "$common/scripts/fxcap/scenarios" scenarios
  --root "$runner_home/work/xfx/xfx/scripts/fxcap/scenarios" scenarios
  --root "$made" scratch --root "$made/work" work)
public=(--root "$runner_home" home --root "$runner_home/.bun/bin/bun" bun)
bun=$(command -v bun || true)
case $bun in "" | "$HOME"/*) ;; *) public+=(--root "$bun" bun) ;; esac
echo "tree $(git rev-parse --short=8 HEAD); roots.py sha256 $(shasum -a 256 "$roots_py" | cut -c1-12)"
echo "tracked files $(git ls-files | wc -l | tr -d ' '), $(git ls-files .prd | wc -l | tr -d ' ') of them under .prd/"
err=${TMPDIR:-/tmp}/lpw-scan.$$
# Each set over the committed captures and receipts (.prd/), then over the
# rest (the source, its tests and fixtures: a line there may hold a shape on
# purpose, marked `local-path-ok`, which roots.py does not read).
for set in machine public; do
  if [ "$set" = machine ]; then args=("${machine[@]}"); else args=("${public[@]}"); fi
  for part in captures-and-receipts rest; do
    if [ "$part" = rest ]; then list=(-- . ':!.prd'); else list=(-- .prd); fi
    git ls-files -z "${list[@]}" | xargs -0 python3 -I "$roots_py" "${args[@]}" --check -- 2>"$err"
    status=$?
    echo "== roots.py --check, the $set roots ($((${#args[@]} / 3))), $part ($(git ls-files "${list[@]}" | wc -l | tr -d ' ') files): exit $status, $(grep -c 'LOCAL-PATH GUARD' "$err") file(s) refused"
    grep 'LOCAL-PATH GUARD' "$err" | sed 's/^LOCAL-PATH GUARD: //' | sort
  done
done
rm -f "$err"
git ls-files -z | python3 -I -c '
import sys
sys.path.insert(0, sys.argv[1])
sys.dont_write_bytecode = True
import roots
if not hasattr(roots, "MARKS"):
    sys.exit("rows behind a mark: this roots.py has no MARKS")
rows = files = 0
for n in sys.stdin.buffer.read().split(b"\0"):
    try:
        data = open(n, "rb").read() if n else b""
    except OSError:
        continue
    if not any(m in data for m in roots.MARKS):
        continue
    k = len(roots.View(data).marked)
    rows, files = rows + k, files + bool(k)
print("rows that open behind a mark in tracked files: %d, in %d file(s)" % (rows, files))
' "$(dirname "$roots_py")"
echo "== scripts/check-no-local-paths.sh"
./scripts/check-no-local-paths.sh 2>&1 | tail -5
