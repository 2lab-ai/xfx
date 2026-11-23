#!/usr/bin/env bash
# LOCAL-PATHS-WRAP after the merge of train 12's head (71d3e02b): the
# dispatcher's five reruns, in <tree> at its head, one at a time. One line
# per step: exit status, seconds, the step; then the verbs test's line from
# each cargo run, and scan.sh's output. Logs in <logs>.
#   rerun.sh <tree> <logs>   (under the suite lock)
set -u
tree=$1 logs=$2
mkdir -p "$logs"
cd "$tree" || exit 2
printf 'head %s\n' "$(git rev-parse HEAD)"
n=0 bad=0
step() {
  local name=$1 start rc
  shift
  n=$((n + 1))
  start=$(date +%s)
  "$@" >"$logs/$(printf '%02d' $n)-$name.log" 2>&1 </dev/null
  rc=$?
  [ "$rc" -eq 0 ] || bad=$((bad + 1))
  printf '%-4s %5ss  %s\n' "$rc" "$(( $(date +%s) - start ))" "$name"
}
step test cargo test --locked
step test-fi cargo test --locked --features fault-injection
step test-capture ./scripts/fxcap/test-capture.sh
step check-no-local-paths ./scripts/check-no-local-paths.sh
step scan bash .prd/tui-phase5/receipts/local-paths-wrap/scan.sh "$tree"
printf 'steps %d, failed %d; tree %s\n' "$n" "$bad" "$( [ -z "$(git status --porcelain)" ] && echo clean || echo dirty)"
for f in "$logs"/01-test.log "$logs"/02-test-fi.log; do
  printf '%s: %s; %s\n' "${f##*/}" "$(grep -h 'the_verbs_scenario_leaves_fx_v0_0_16s_bytes' "$f" | head -1)" \
    "$(grep -c '^test result: ok' "$f") test targets ok, $(grep -c '^test result: FAILED' "$f") failed"
done
printf '03-test-capture: %s\n' "$(tail -3 "$logs/03-test-capture.log" | tr '\n' ' ')"
printf '04-check-no-local-paths: %s\n' "$(tail -1 "$logs/04-check-no-local-paths.log")"
echo "05-scan:"
cat "$logs/05-scan.log"
