#!/usr/bin/env bash
# LOCAL-PATHS: the lines runs-scan.txt gives after runs_scan.py's table for a
# sweep: capture files holding a placeholder, each run's `roots:` line with
# its widths masked, runs roots.py refused, and what follows the binaries'
# TMPDIR names in its captures. Counts and placeholders only, never a path.
#   runs-tail.sh <label> <sweep's runs/ directory>
set -u
label=$1 runs=$2
cd "$runs" || exit 2
ph=$(grep -rlE '<((work|scratch|scenarios|bun|tmp|home)(:real)?_*|[wscbthWSCBTH]_+)>' . | grep -v -E '/(capture\.log|status\.json)$' | wc -l | tr -d ' ')
rl=$(grep -rh '^roots:' . | sed -E 's/=[0-9]+/=N/g' | sort -u)
gd=$(grep -rl 'LOCAL-PATH GUARD' . | wc -l | tr -d ' ')
fol=$(grep -r -h -o -a -E 'x?fxtmp(.|$)' . | sed -E 's/^x?fxtmp//' | LC_ALL=C sort | uniq -c | sed -E 's/^ *([0-9]+) (.*)$/\1 `\2`/; s/^ *([0-9]+) *$/\1 (the line'"'"'s end)/' | paste -sd ',' - | sed 's/,/, /g')
printf '%s: capture files holding a placeholder: %s\n' "$label" "$ph"
printf "%s: each run's roots line (widths masked here): %s\n" "$label" "$rl"
printf '%s: runs refused by roots.py (LOCAL-PATH GUARD in any file of runs/): %s\n' "$label" "$gd"
printf '%s: what follows fxtmp or xfxtmp in its captures: %s\n' "$label" "${fol:-nothing}"
