#!/usr/bin/env bash
# PERM-EARLY compat: older binaries on a session whose saved-session rules
# this build wrote before the session's first prompt (permissions.json in the
# session directory before its log). Loopback fake gateway, fake key, scratch
# HOME. usage: compat.sh <run-dir> <session-id> <preview-port> <base-port> <this-xfx> <base-xfx> <preview-xfx>
set -euo pipefail
run=$1 id=$2 port=$3 port2=$4 this=$5 base=$6 preview=$7
here=$(cd "$(dirname "$0")" && pwd)
home=$here/home
rm -rf "${home:?}"
cp -R "$run/home" "$home"
work=$run/work
dir=$home/.xfx/sessions/$id
gw="XFX_GATEWAY_URL=http://127.0.0.1:$port/v4/ai/language-model"
x() { local bin=$1; shift; (cd "$work" && env -i HOME="$home" PATH=/usr/bin:/bin LANG=en_US.UTF-8 TERM=dumb AI_GATEWAY_API_KEY=fake-key-not-real XFX_MODEL=fake/model-a "$gw" TMPDIR="$here/tmp" "$bin" "$@"); }
mkdir -p "$here/tmp"
hashes() { (cd "$dir" && shasum -a 256 events.jsonl session.json permissions.json); }
kinds() { python3 -I -c 'import json,sys; print([json.loads(l)["event"]["kind"] for l in open(sys.argv[1])])' "$dir/events.jsonl"; }

echo "# the session this build wrote ($(cd "$(dirname "$this")/../.." && git rev-parse --short HEAD)), copied from the capture's HOME"
echo "files: $(cd "$dir" && ls | tr '\n' ' ')"
echo "kinds: $(kinds)"
echo "permissions.json: $(python3 -I -c 'import json,sys; s=json.load(open(sys.argv[1])); print({"schema_version":s["schema_version"],"next_generation":s["next_generation"],"rules":[{k:r[k] for k in ("id","kind","display_identity","decision","generation")} for r in s["rules"]]})' "$dir/permissions.json")"
echo "permissions.json was made before the log: $(python3 -I -c '
import json,os,sys
d=sys.argv[1]
b=lambda n: os.stat(os.path.join(d,n)).st_birthtime
print(b("permissions.json") <= b("events.jsonl"), "(birth times)")' "$dir")"
echo
echo "# preview, built from \`git archive 14ad0420\` with XFX_BUILD_CHANNEL=preview and XFX_BUILD_REVISION=14ad04202fe5, as .github/workflows/preview.yml:225-226 stamps it"
echo "# xfx status | grep build_"
x "$preview" status | grep build_ || true
echo "# sha256 of the binary"
shasum -a 256 "$preview" | cut -d' ' -f1
before=$(hashes)
echo
echo "# preview: xfx session --id <id>"
x "$preview" session --id "$id" && echo "exit=0" || echo "exit=$?"
[[ $(hashes) == "$before" ]] && echo "log, manifest and permissions.json unchanged by the read" || echo "CHANGED by the read"
echo
echo "# preview: xfx sessions"
x "$preview" sessions && echo "exit=0" || echo "exit=$?"
echo
head_bytes=$(wc -c < "$dir/events.jsonl" | tr -d ' ')
cp "$dir/events.jsonl" "$here/events.before"
rules_before=$(shasum -a 256 "$dir/permissions.json" | cut -d' ' -f1)
echo "# preview: xfx ask --resume-id <id> 'one more question'"
x "$preview" ask --resume-id "$id" 'one more question' && echo "exit=0" || echo "exit=$?"
python3 -I -c '
import sys
a=open(sys.argv[1],"rb").read(); b=open(sys.argv[2],"rb").read()
print("the new build'"'"'s bytes are the head of the log:", b.startswith(a))' "$here/events.before" "$dir/events.jsonl"
echo "appended by the preview: $(python3 -I -c 'import json,sys; print([json.loads(l)["event"]["kind"] for l in open(sys.argv[1]).read().splitlines()[int(sys.argv[2]):]])' "$dir/events.jsonl" "$(wc -l < "$here/events.before" | tr -d ' ')")"
[[ $(shasum -a 256 "$dir/permissions.json" | cut -d' ' -f1) == "$rules_before" ]] && echo "permissions.json unchanged (the preview predates saved-session rules: it neither reads nor writes the file)"
echo
echo "# base ($(cd "$(dirname "$base")/../.." && git rev-parse --short HEAD)): xfx session --id <id>"
x "$base" session --id "$id" | grep -E 'history_turns|permission_mode' && echo "exit=0"
echo
echo "# base: xfx ask --resume-id <id> 'read it again' -- the model calls read_file a.txt"
gw="XFX_GATEWAY_URL=http://127.0.0.1:$port2/v4/ai/language-model"
x "$base" ask --resume-id "$id" 'read it again' && echo "exit=0" || echo "exit=$?"
echo
echo "# this build: xfx session --id <id>"
x "$this" session --id "$id" | grep -E 'history_turns|permission_mode' && echo "exit=0"
