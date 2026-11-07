#!/bin/sh
# The old-session probe's binary: the base build on the first launch, the
# head build on every launch after it (capture.sh's `relaunch`). Each launch
# is logged with the build it ran.
mark=<probe>/launched
log=<probe>/launches.log
if [ -e "$mark" ]; then
  echo "launch 2+: xfx-head-95669160 $*" >> "$log"
  exec <builds>/xfx-head-95669160 "$@"
fi
: > "$mark"
echo "launch 1: xfx-base-e6cde019 $*" >> "$log"
exec <builds>/xfx-base-e6cde019 "$@"
