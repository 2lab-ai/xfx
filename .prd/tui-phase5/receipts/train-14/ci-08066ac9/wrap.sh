#!/usr/bin/env bash
# Train 14, ci-08066ac9 (new): sweep.py's capture, as FXCAP_SWEEP_CAPTURE
# names it -- the tree's own capture.sh run unchanged ($REAL_CAPTURE), then,
# before sweep.py clears the unit's TMPDIR, the names of the run directories
# it holds (the sweep's shared `fxsw.` one, this capture's own `fxcap.` one)
# appended to $NAMES_LOG, a scratch file outside the sweep's out directory
# and outside this folder. read.py reads those names for their letters and
# writes none of them here.
# usage (by sweep.py): wrap.sh <side> <scenario> <size> <outdir>
bash "${REAL_CAPTURE:?}" "$@"
rc=$?
for d in "$TMPDIR"/fxcap.*; do
  [ -d "$d" ] && printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$(basename "$(dirname "$TMPDIR")")" "$(basename "$d")" >> "${NAMES_LOG:?}"
done
exit "$rc"
