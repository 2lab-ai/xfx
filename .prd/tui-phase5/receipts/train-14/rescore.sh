#!/usr/bin/env bash
# Train 14 (new; train 13 ran the same command inline): a sweep rescored
# with the code head's ledger -- a copy of <scratch>/<from> as
# <scratch>/<to>, classified again by the head clone's sweep.sh --rescore
# (its ledger, its scenarios' scoring). The output is <scratch>/<to>.rescore.run.
# Two uses: the base's sweep (sw-base -> ref-base) and the base's units'
# sweep (sw-base-u -> ref-base-u), so each pair is scored by one ledger.
# usage: rescore.sh <scratch> <head-clone> <from> <to>
set -u
T=$1 hclone=$2 from=$3 to=$4
if [ -e "$T/$to" ]; then echo "$to exists; not redone"; exit 2; fi
cp -R "$T/$from" "$T/$to" || exit 2
bash "$hclone/scripts/fxcap/sweep.sh" --rescore --out "$T/$to" > "$T/$to.rescore.run" 2>&1
echo "exit $?" >> "$T/$to.rescore.run"
tail -3 "$T/$to.rescore.run"
