#!/usr/bin/env bash
# fakegw on its own, asked by curl, for what no binary here reaches: the
# member-scope budget route (/ai-gateway/usage-report), a path nothing knows,
# and the Codex wire's `reset` and `refuse`. Run from the repository root;
# writes nothing outside a fresh directory under TMPDIR.
set -u
root=$(pwd)
dir=$(mktemp -d "${TMPDIR:-/tmp}/fakegw-selftest.XXXXXX")
printf '[{"reset":true},{"refuse":1500},{"chunks":["after"]}]\n' > "$dir/turns.json"
cp scripts/fxcap/scenarios/h1-usage-budget.json "$dir/usage.json"
FAKEGW_USAGE=$dir/usage.json bun "$root/scripts/fxcap/fakegw.ts" 0 "$dir/turns.json" > "$dir/gw.log" 2>&1 &
gw=$!
until grep -q listening "$dir/gw.log" 2>/dev/null; do sleep 0.1; done
port=$(sed -n 's/.*listening \([0-9]*\).*/\1/p' "$dir/gw.log" | head -1)
ask() {
  echo "\$ curl $*" | sed "s/$port/<port>/g"
  curl -sS -m 5 -w ' [%{http_code}]' "$@" 2>&1 | sed "s/$port/<port>/g"
  echo
}
ask -H 'x-vercel-ai-gateway-team: team_fake' -H 'authorization: Bearer not-a-real-token' "http://127.0.0.1:$port/ai-gateway/usage-report?teamId=team_fake&scope=user&start=2026-10-12&end=2026-10-12"
ask "http://127.0.0.1:$port/.well-known/openid-configuration"
ask -X POST -d '{}' "http://127.0.0.1:$port/codex/responses"
sleep 0.3
ask -X POST -d '{}' "http://127.0.0.1:$port/codex/responses"
sleep 0.3
ask "http://127.0.0.1:$port/codex/models"
sleep 1.6
ask -X POST -d '{}' "http://127.0.0.1:$port/codex/responses"
kill "$gw"
wait "$gw" 2>/dev/null
echo "--- gateway log"
sed "s/$port/<port>/g" "$dir/gw.log"
