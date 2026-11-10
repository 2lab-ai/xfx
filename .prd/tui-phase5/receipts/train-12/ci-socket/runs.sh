#!/bin/bash
# The runs behind this receipt, as they were made (each under the suite lock,
# one at a time). Every path is an argument; nothing here names a machine.
# They ran as two suite-lock calls: the RED and targeted GREEN runs on the
# working tree that became 44bb45e6, the rest at 44bb45e6 itself. Each run
# writes <out>/<name>.raw; this folder keeps it as <name>.txt, its local
# paths written as placeholders.
#
# usage: runs.sh <repo> <out> <runner-length tmpdir> <gate tmpdir> <short --tmp> <fx v0.0.16> <red tree>
#   <runner-length tmpdir>  49 bytes with its trailing slash, through a link
#                           (/tmp -> /private/tmp, as the runner's /var is), so
#                           57 resolved: the CI runner's TMPDIR, byte for byte
#   <gate tmpdir>           the local gate's TMPDIR, 23 bytes, no link
#   <short --tmp>           the sweep's --tmp, as CI's $RUNNER_TEMP is short
#   <red tree>              scripts/fxcap at f53606d1 (`git archive f53606d1 scripts/fxcap`)
set -u
repo=$1 out=$2 long=$3 short=$4 rt=$5 fx=$6 red=$7/scripts/fxcap
T1=RunRoots.test_every_file_a_run_writes_names_its_roots_by_placeholder_at_any_tmpdir
T2=Refusals.test_a_tmpdir_too_long_for_the_socket
run() { # <name> <tmpdir> <command>...
  local name=$1 t=$2; shift 2
  { echo "== $name: TMPDIR $(printf %s "$t" | wc -c | tr -d ' ') bytes; $*"; echo "start $(date -u +%FT%TZ)"; } > "$out/$name.raw"
  ( cd "$repo" && TMPDIR=$t FX_AUTO_UPGRADE=0 "$@" ) >> "$out/$name.raw" 2>&1
  echo "exit=$?" >> "$out/$name.raw"
  echo "end $(date -u +%FT%TZ)" >> "$out/$name.raw"
}
# RED: the harness at f53606d1.
run red-one-long "$long" "$red/test-capture.sh" -v "$T1"
run red-full-long "$long" "$red/test-capture.sh"
# GREEN: the harness at the fix.
run green-targeted-long "$long" ./scripts/fxcap/test-capture.sh -v "$T1" "$T2"
run green-targeted-short "$short" ./scripts/fxcap/test-capture.sh -v "$T1" "$T2"
run green-full-long "$long" ./scripts/fxcap/test-capture.sh
run green-full-short "$short" ./scripts/fxcap/test-capture.sh
# The sweep job's later steps at the runner-length TMPDIR.
run sweep-tests-long "$long" ./scripts/fxcap/test-sweep.sh
run build-long "$long" cargo build --locked
mkdir -p "$out/bin" "$rt"
cp "$repo/target/debug/xfx" "$out/bin/xfx-head"
"$out/bin/xfx-head" --version > "$out/xfx-version.raw" 2>&1
shasum -a 256 "$fx" | cut -d' ' -f1 > "$out/oracle-sha-before.raw"
run ci-sweep-sample-long "$long" ./scripts/fxcap/ci-sweep.sh --base HEAD --core "$(dirname "$0")/sample-core.txt" \
  --fx "$fx" --pin v0.0.16 --xfx "$out/bin/xfx-head" --base-xfx "$out/bin/xfx-head" \
  --jobs 2 --tmp "$rt" --out "$out/sweep"
shasum -a 256 "$fx" | cut -d' ' -f1 > "$out/oracle-sha-after.raw"
