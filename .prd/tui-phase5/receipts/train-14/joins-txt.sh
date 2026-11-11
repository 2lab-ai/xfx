#!/usr/bin/env bash
# Train 14 (new): joins.txt, through redacted.py -- join checks 1 and 2 as
# joins.sh printed them (each part's lines: the TMPDIR's length, the load,
# fx's sha256 before and after, the pass line; each walk run's load), then
# joins_txt.py's reading of the head's runs against the head's main sweep,
# then join check 1's TMPDIR part on the base's build (dcfa5146, the base
# sweep's clone) against the base's main sweep.
# usage: joins-txt.sh <scratch>   (its joins.out, joins/, joins-base.out, joins-base/, sw-head, sw-base)
set -u
T=$1
here=$(cd "$(dirname "$0")" && pwd)
root=/tmp/claude-$(id -u)/
dir_of() { local pad=$(($1 - ${#root} - 1)); printf '%s%s/' "$root" "$(printf 'j%0*d' $((pad - 1)) 0)"; }   # joins.sh's rule
r24=$(dir_of 24) r49=$(dir_of 49)
{
  echo "# join checks 1 and 2 at the code head, as joins.sh printed them (one hold of the suite lock, after the head's main sweep)"
  cat "$T/joins.out"
  echo
  python3 -I -B "$here/joins_txt.py" "$T/joins" "$T/sw-head" "$r24" "$r49"
  echo
  echo "# join check 1's TMPDIR part on the base's build, as joins.sh printed it"
  cat "$T/joins-base.out"
  echo
  python3 -I -B "$here/joins_txt.py" "$T/joins-base" "$T/sw-base" "$r24" "$r49"
} 2>&1 | python3 -I -B "$here/redacted.py" "$here/joins.txt" "$T=<scratch>"
