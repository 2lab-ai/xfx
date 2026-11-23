#!/usr/bin/env bash
# One suite-lock slot: mutant M14 on a throwaway clone at <rev>, then the gate on another clean clone at <rev>.
set -uo pipefail
repo=$1 rev=$2 W=$3 O=$4
rm -rf "$W/mut" "$O"; mkdir -p "$O"
git clone -q --no-local "$repo" "$W/mut" && git -C "$W/mut" checkout -q --detach "$rev" || exit 1
( cd "$W/mut" && python3 -I .prd/tui-phase5/receipts/slash-mixed-skills/mutants.py "$W/mut" M14 ) > "$O/mutants-m14.txt" 2>&1
echo "mutants M14 rc=$?; porcelain after: $(git -C "$W/mut" status --porcelain | wc -l | tr -d ' ')" >> "$O/mutants-m14.txt"
rm -rf "$W/mut"
bash "$W/gate.sh" "$repo" "$rev" "$W/g" "$O/gate"
