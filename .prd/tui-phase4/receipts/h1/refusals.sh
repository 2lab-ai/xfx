#!/usr/bin/env bash
# Every bad argument to an H1 verb, one scenario each, run on xfx: each must
# exit 2 before the binary is launched (the last two once the fake gateway
# has said its port, which is when a placeholder is written in). Run from the
# repository root; the scenarios are written to a fresh directory under
# TMPDIR.
set -u
root=$(pwd)
dir=$(mktemp -d "${TMPDIR:-/tmp}/fxcap-refusals.XXXXXX")
printf '{}\n' > "$dir/ok.json"
printf '{"mcp":"@MCPPORT@"}\n' > "$dir/mcpport.json"
n=0
while IFS= read -r line; do
  n=$((n + 1))
  printf '%s\nwait 1\n' "$line" > "$dir/bad$n.cap"
  FXCAP_SCENARIOS=$dir "$root/scripts/fxcap/capture.sh" xfx "bad$n" 24x80 "$dir/out" > "$dir/out.txt" 2>&1
  code=$?
  printf '%s  %-40s %s\n' "$code" "$line" "$(tail -1 "$dir/out.txt" | sed "s#$dir#<dir>#g; s#${TMPDIR:-/tmp}/fxcap\.[A-Za-z0-9]*#<run>#g")"
done <<'LINES'
resize 0x80
resize 24
resize 24x80x2
resize
signal SIGKILL
signal INT
signal
rehomefile /etc/x ok.json
rehomefile ../x ok.json
rehomefile .fx/a/../../x ok.json
rehomefile .fx/settings.json missing.json
rehomefile .fx/settings.json
clock soon
clock
usage missing.json
usage
mcphttp sleepy
mcphttp
models2 ok.json
models2 missing.json 100
models2 ok.json soon
modelsdelay soon
jobctl now
sockets
env X=@MCPPORT@
homefile .xfx/m.json mcpport.json
LINES
