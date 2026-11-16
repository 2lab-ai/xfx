#!/usr/bin/env bash
# Train 16: train 15's selections.sh, its selections this train's: the sweep
# selection, as both sweeps read it (selection-sweep.txt: selection-all.txt
# then selection-units.txt, copied here as all three), made again from this
# folder's scripts at this revision, the scenario names compared.
#   selection-all.txt   = selection.py over the six merges and the join
#                         commit (the first re-point c637ed4c listed, not
#                         selected), then its --extra for each re-point
#                         (c637ed4c and 15c9744b), each reading the file as
#                         it stood before it, as they ran
#   selection-units.txt = selection_units.py: the scenarios the units' seven
#                         committed head sweeps took that selection-all.txt
#                         does not (PROFILE-REFUSE-TYPED-KEYS,
#                         PROFILE-JSON-LIMITS, TEST-ENV-ISOLATION and
#                         LOCAL-PATHS-WRAP committed none)
# Output: selections.txt, through redacted.py.
# usage: selections.sh <repo> <scratch>   (<scratch>/selection-*.txt, as swept)
set -u
repo=$1 T=$2
here=$(cd "$(dirname "$0")" && pwd)
U=.prd/tui-phase5/receipts
P="python3 -I -B $here"
M="b2d044c9 e8b004a5 849f5017 dd2ac93d 71787971 1bce5348 622b2865"
for f in selection-all.txt selection-units.txt selection-sweep.txt; do cp "$T/$f" "$here/$f"; done
names() { grep -v '^#' "$1" | grep -v '^$' | sort; }
{
  echo "# the selection made again here, its scenario names against the swept file's"
  $P/selection.py "$repo" c637ed4c $M > "$T/selection-all-redo.txt" 2>&1
  $P/selection.py --extra "$repo" c637ed4c $M "$T/selection-all-redo.txt" > "$T/selection-extra1.redo" 2>&1
  cat "$T/selection-extra1.redo" >> "$T/selection-all-redo.txt"
  $P/selection.py --extra "$repo" 15c9744b $M "$T/selection-all-redo.txt" > "$T/selection-extra2.redo" 2>&1
  cat "$T/selection-extra2.redo" >> "$T/selection-all-redo.txt"
  echo "selection-all.txt: $(names "$T/selection-all.txt" | wc -l | tr -d ' ') names swept; made again $(names "$T/selection-all-redo.txt" | wc -l | tr -d ' '); $(cmp -s <(names "$T/selection-all.txt") <(names "$T/selection-all-redo.txt") && echo 'the same names' || echo 'NAMES DIFFER')"
  $P/selection_units.py "$repo" 2b78aab6 866747ff "$T/selection-all.txt" \
    bacb1052:$U/profile-refuse-at-launch/sweep-r3-head/summary.md bacb1052:$U/permission-mode-case/sweep/head/summary.md \
    32eb858f:$U/slash-leading-ws/sweep-head.md 32eb858f:$U/slash-arg-skills/sweep-head.md \
    32eb858f:$U/slash-mixed-skills/sweep-head.md ca496c10:$U/approval-paste-spaces/sweep-head.md \
    6d94fbc0:$U/meta-prefix-decode/sweep-head.md > "$T/selection-units.redo" 2>&1
  echo "selection-units.txt: $(names "$T/selection-units.txt" | wc -l | tr -d ' ') names swept; made again $(names "$T/selection-units.redo" | wc -l | tr -d ' '); $(cmp -s "$T/selection-units.txt" "$T/selection-units.redo" && echo 'the same file, byte for byte' || echo 'DIFFERENT')"
  echo "no name in both: $(comm -12 <(names "$T/selection-all.txt") <(names "$T/selection-units.txt") | wc -l | tr -d ' ')"
  echo "selection-sweep.txt is the two, in order: $(cat "$T/selection-all.txt" "$T/selection-units.txt" | cmp -s - "$T/selection-sweep.txt" && echo yes || echo NO)"
  for c in 866747ff 2b78aab6; do
    python3 -I -B - "$repo" "$c" "$T/selection-sweep.txt" <<'PY'
import fnmatch, subprocess, sys
repo, c, sel = sys.argv[1:]
have = sorted(p.rsplit("/", 1)[-1][:-4] for p in subprocess.run(["git", "-C", repo, "ls-tree", "--name-only", f"{c}:scripts/fxcap/scenarios"], capture_output=True, text=True).stdout.split() if p.endswith(".cap"))
g = [l.strip() for l in open(sel) if l.strip() and not l.startswith("#")]
print(f"{c}: {len(have)} scenarios, {sum(1 for h in have if any(fnmatch.fnmatch(h, x) for x in g))} of them selected")
PY
  done
} 2>&1 | python3 -I -B "$here/redacted.py" "$here/selections.txt" "$repo=<repo>" "$T=<scratch>"
