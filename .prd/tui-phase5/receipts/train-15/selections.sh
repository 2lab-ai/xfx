#!/usr/bin/env bash
# Train 15 (train 14's selections.sh, its selections this train's): the sweep
# selection, as both sweeps read it (selection-sweep.txt: selection-all.txt
# then selection-units.txt, copied here as all three), made again from this
# folder's scripts at this revision, the scenario names compared.
#   selection-all.txt   = selection.py over the five merges and the join of
#                         main (the re-point da8e4c1a listed, not selected),
#                         then its --extra for each re-point (da8e4c1a and
#                         2c575f52), each reading the file as it stood
#                         before it, as they ran (selection.py reads a first
#                         selection from an argument ending in .txt)
#   selection-units.txt = selection_units.py: the scenarios the units' own
#                         eleven committed head sweeps took that
#                         selection-all.txt does not (CATALOG-WAIT-CLOCK and
#                         TILDE-EMPTY-MODIFIER committed none)
# Output: selections.txt, through redacted.py.
# usage: selections.sh <repo> <scratch>   (<scratch>/selection-*.txt, as swept)
set -u
repo=$1 T=$2
here=$(cd "$(dirname "$0")" && pwd)
U=.prd/tui-phase5/receipts
P="python3 -I -B $here"
M="2ff302c5 ed92e515 b10ca771 88e83a68 07eefa89 a14df710"
for f in selection-all.txt selection-units.txt selection-sweep.txt; do cp "$T/$f" "$here/$f"; done
names() { grep -v '^#' "$1" | grep -v '^$' | sort; }
{
  echo "# the selection made again here, its scenario names against the swept file's"
  $P/selection.py "$repo" da8e4c1a $M > "$T/selection-all-redo.txt" 2>&1
  $P/selection.py --extra "$repo" da8e4c1a $M "$T/selection-all-redo.txt" > "$T/selection-extra1.redo" 2>&1
  cat "$T/selection-extra1.redo" >> "$T/selection-all-redo.txt"
  $P/selection.py --extra "$repo" 2c575f52 $M "$T/selection-all-redo.txt" > "$T/selection-extra2.redo" 2>&1
  cat "$T/selection-extra2.redo" >> "$T/selection-all-redo.txt"
  echo "selection-all.txt: $(names "$T/selection-all.txt" | wc -l | tr -d ' ') names swept; made again $(names "$T/selection-all-redo.txt" | wc -l | tr -d ' '); $(cmp -s <(names "$T/selection-all.txt") <(names "$T/selection-all-redo.txt") && echo 'the same names' || echo 'NAMES DIFFER')"
  $P/selection_units.py "$repo" 2c575f52 e2fc7e94 "$T/selection-all.txt" \
    df45025c:$U/decoder-tilde-home-end/sweep-head.md df45025c:$U/modified-page-keys/sweep-r2-head.md \
    df45025c:$U/modified-arrows-screens/sweep-head.md df45025c:$U/composer-paragraph-move/sweep-head.md \
    df45025c:$U/composer-super-arrows/sweep-head.md fffb3cfe:$U/fulltranscript-response-header/sweep-head2.md \
    7c9dfb2e:$U/approval-readiness-settled/sweep-head.md 7c9dfb2e:$U/approval-draft-keys/sweep-head.md \
    7c9dfb2e:$U/approval-ctrl-j/sweep-head.md 7c9dfb2e:$U/rule-confirm-keys/sweep-head.md \
    b5cb6f46:$U/question-ctrl-j/sweep-head.md > "$T/selection-units.redo" 2>&1
  echo "selection-units.txt: $(names "$T/selection-units.txt" | wc -l | tr -d ' ') names swept; made again $(names "$T/selection-units.redo" | wc -l | tr -d ' '); $(cmp -s "$T/selection-units.txt" "$T/selection-units.redo" && echo 'the same file, byte for byte' || echo 'DIFFERENT')"
  echo "no name in both: $(comm -12 <(names "$T/selection-all.txt") <(names "$T/selection-units.txt") | wc -l | tr -d ' ')"
  echo "selection-sweep.txt is the two, in order: $(cat "$T/selection-all.txt" "$T/selection-units.txt" | cmp -s - "$T/selection-sweep.txt" && echo yes || echo NO)"
  for c in e2fc7e94 2c575f52; do
    python3 -I -B - "$repo" "$c" "$T/selection-sweep.txt" <<'PY'
import fnmatch, subprocess, sys
repo, c, sel = sys.argv[1:]
have = sorted(p.rsplit("/", 1)[-1][:-4] for p in subprocess.run(["git", "-C", repo, "ls-tree", "--name-only", f"{c}:scripts/fxcap/scenarios"], capture_output=True, text=True).stdout.split() if p.endswith(".cap"))
g = [l.strip() for l in open(sel) if l.strip() and not l.startswith("#")]
print(f"{c}: {len(have)} scenarios, {sum(1 for h in have if any(fnmatch.fnmatch(h, x) for x in g))} of them selected")
PY
  done
} 2>&1 | python3 -I -B "$here/redacted.py" "$here/selections.txt" "$repo=<repo>" "$T=<scratch>"
