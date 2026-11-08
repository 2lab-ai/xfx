#!/bin/bash
# usage: loops.sh <label> <fi-binary> <plain-binary> <n>
# The original pty test n times on each build under 18 busy loops (one per
# core), one build after the other. Run under SUITE_LOCK_EXCLUSIVE=1.
set -u
label=$1 fi=$2 plain=$3 n=$4
export TMPDIR=/private/tmp/claude-501/cnl
cd <repo>
echo "# $label  $(date -u +%FT%TZ)  cores=$(sysctl -n hw.ncpu)"
echo "## fault-injection build"
perl <scratch>/loaded.pl 18 "$n" "$fi" ctrl_c_stops_a_compaction_that_is_waiting_for_its_notes
echo "## plain build"
perl <scratch>/loaded.pl 18 "$n" "$plain" ctrl_c_stops_a_compaction_that_is_waiting_for_its_notes
echo "# done $(date -u +%FT%TZ)"
