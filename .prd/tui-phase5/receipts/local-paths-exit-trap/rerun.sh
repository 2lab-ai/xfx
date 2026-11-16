#!/usr/bin/env bash
# LOCAL-PATHS-EXIT-TRAP: the gate's one failing test, alone, three times, on
# the gate's clone, inside one hold of the suite lock.
#
#   suite-lock.sh rerun.sh <clone> <test name> [<features>]
set -u
clone=$1 name=$2 features=${3:-}
cd "$clone" || exit 2
echo "# head $(git rev-parse --short HEAD); cargo test --locked ${features:+--features $features }--lib $name -- --exact"
for i in 1 2 3; do
	load=$(sysctl -n vm.loadavg | awk '{print $2}')
	cargo test --locked ${features:+--features "$features"} --lib "$name" -- --exact >"$TMPDIR/rerun-$i.log" 2>&1
	rc=$?
	printf 'run %d: exit %d, load %s, %s\n' "$i" "$rc" "$load" "$(grep -E '^test result:' "$TMPDIR/rerun-$i.log" | head -1)"
done
