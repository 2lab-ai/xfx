#!/usr/bin/env bash
# The probe's clones: a git clone --no-local at <rev>, the probe appended to
# src/tui/shell.rs's `mod tests` (before the file's last `}`).
# usage: prep.sh <repo> <rev> <probe.rs> <clone>
set -eu
repo=$1 rev=$2 probe=$3 clone=$4
rm -rf "$clone"
git clone -q --no-local "$repo" "$clone"
git -C "$clone" checkout -q "$rev"
python3 -I -B - "$clone/src/tui/shell.rs" "$probe" <<'PY'
import sys
path, probe = sys.argv[1], sys.argv[2]
text = open(path).read()
assert text.endswith("}\n")
open(path, "w").write(text[:-2] + "\n" + open(probe).read().rstrip("\n") + "\n}\n")
PY
echo "$(basename "$clone"): $(git -C "$clone" log --oneline -1 | cut -c1-60); probe $(basename "$probe") appended"
