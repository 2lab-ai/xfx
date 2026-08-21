#!/usr/bin/env bash
#
# End-to-end smoke test of a built fxr binary.
#
#   scripts/smoke.sh <path-to-fxr> [evidence-dir]
#
# It drives the real executable through the paths a release has to survive:
# help and status, a content-only answer, a multi-step turn that reads a file,
# edits it, runs an admitted command and is refused a destructive one, the
# session lifecycle including resume, rebind, and `--no-save`, and finally the
# interactive shell on a real pseudoterminal.
#
# Two properties are not negotiable:
#
#   * It never touches a live credential or the network. Every model response
#     comes from a fake Gateway this script starts on a loopback port, and the
#     only credential in play is a literal that must never appear in output.
#   * It writes nothing into the repository. All state -- the profile home, the
#     workspace it edits, and every captured stream -- lives under the evidence
#     directory, which defaults to a temporary path and is printed at the end.
#
# Requirements: bash, python3 (present on every supported platform and on the
# CI images). The fake Gateway and the pty driver are written out as real files
# under the evidence directory, so a failure can be reproduced by hand.

set -euo pipefail

binary="${1:-}"
if [ -z "$binary" ]; then
	printf 'usage: %s <path-to-fxr> [evidence-dir]\n' "$0" >&2
	exit 2
fi
if [ ! -x "$binary" ]; then
	printf 'smoke: %s is not an executable binary\n' "$binary" >&2
	exit 2
fi
binary="$(cd "$(dirname "$binary")" && pwd)/$(basename "$binary")"

if ! command -v python3 >/dev/null 2>&1; then
	printf 'smoke: python3 is required (it hosts the fake Gateway and the pty driver)\n' >&2
	exit 2
fi

evidence="${2:-${TMPDIR:-/tmp}/fxr-smoke-$(date +%Y%m%dT%H%M%S)-$$}"
mkdir -p "$evidence"
evidence="$(cd "$evidence" && pwd)"

# A credential-shaped literal that is not a credential. Every captured stream is
# scanned for it at the end: a product that prints its key once will print
# someone's real key eventually.
readonly FAKE_KEY="fxr-smoke-key-must-not-appear-in-output"

failures=0
checks=0

pass() {
	checks=$((checks + 1))
	printf '  ok    %s\n' "$1"
}

fail() {
	checks=$((checks + 1))
	failures=$((failures + 1))
	printf '  FAIL  %s\n' "$1" >&2
}

# Asserts that `haystack` (a file) contains `needle`.
expect_contains() {
	local file="$1" needle="$2" what="$3"
	if grep -qF -- "$needle" "$file"; then
		pass "$what"
	else
		fail "$what (looked for '$needle' in $file)"
	fi
}

expect_absent() {
	local file="$1" needle="$2" what="$3"
	if grep -qF -- "$needle" "$file"; then
		fail "$what (found '$needle' in $file)"
	else
		pass "$what"
	fi
}

expect_status() {
	local actual="$1" wanted="$2" what="$3"
	if [ "$actual" = "$wanted" ]; then
		pass "$what"
	else
		fail "$what (exit $actual, wanted $wanted)"
	fi
}

# ---------------------------------------------------------------------------
# the fake Gateway and the pty driver
# ---------------------------------------------------------------------------

helpers="$evidence/helpers"
mkdir -p "$helpers"

cat >"$helpers/fake_gateway.py" <<'PYTHON'
"""A scripted stand-in for the Vercel AI Gateway, on a loopback port.

It answers each POST with the next reply from a JSON script, in order, as a
`text/event-stream` body, and records every request it received. A request past
the end of the script is answered 500, so an unexpected extra round trip fails
the run instead of hanging it.
"""

import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer

script_path, port_path, requests_path = sys.argv[1], sys.argv[2], sys.argv[3]

with open(script_path, encoding="utf-8") as handle:
    replies = json.load(handle)

lock = threading.Lock()
served = 0


def render(events):
    body = "".join(f"data: {json.dumps(event)}\n\n" for event in events)
    return body + "data: [DONE]\n\n"


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_POST(self):  # noqa: N802 - the name is the framework's
        global served
        length = int(self.headers.get("content-length", "0"))
        body = self.rfile.read(length).decode("utf-8", "replace")
        with lock:
            index = served
            served += 1
            with open(requests_path, "a", encoding="utf-8") as handle:
                handle.write(
                    json.dumps(
                        {
                            "index": index,
                            "path": self.path,
                            "headers": {k.lower(): v for k, v in self.headers.items()},
                            "body": body,
                        }
                    )
                    + "\n"
                )
        if index >= len(replies):
            payload = b'{"error":"fake gateway: unscripted request"}'
            self.send_response(500)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
            return
        reply = replies[index]
        if "status" in reply:
            payload = json.dumps(reply.get("body", {})).encode("utf-8")
            self.send_response(reply["status"])
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
            return
        payload = render(reply["events"]).encode("utf-8")
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.send_header("cache-control", "no-cache")
        self.send_header("content-length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, *_args):
        pass


server = HTTPServer(("127.0.0.1", 0), Handler)
with open(port_path, "w", encoding="utf-8") as handle:
    handle.write(str(server.server_address[1]))
server.serve_forever()
PYTHON

cat >"$helpers/pty_shell.py" <<'PYTHON'
"""Drives the interactive shell on a real pseudoterminal.

A pipe cannot be used: `fxr` refuses to open a shell without a terminal, which
is itself one of the things this checks. The transcript is written verbatim so
the run leaves evidence of what the terminal actually received.
"""

import os
import pty
import re
import select
import sys
import time

binary, workspace, transcript_path = sys.argv[1], sys.argv[2], sys.argv[3]
env_pairs = sys.argv[4:]

pid, fd = pty.fork()
if pid == 0:
    os.chdir(workspace)
    for pair in env_pairs:
        key, _, value = pair.partition("=")
        os.environ[key] = value
    os.execv(binary, [binary])

captured = bytearray()
deadline_seconds = 30.0


def pump(until, timeout=deadline_seconds):
    limit = time.time() + timeout
    while time.time() < limit:
        if until.search(captured.decode("utf-8", "replace")):
            return True
        ready, _, _ = select.select([fd], [], [], 0.1)
        if not ready:
            continue
        try:
            chunk = os.read(fd, 4096)
        except OSError:
            break
        if not chunk:
            break
        captured.extend(chunk)
    return bool(until.search(captured.decode("utf-8", "replace")))


def send(text):
    os.write(fd, text.encode("utf-8"))


problems = []


def require(condition, what):
    if not condition:
        problems.append(what)


require(pump(re.compile(r"> ")), "the shell printed a prompt")
send("/help\r")
require(pump(re.compile(r"/version")), "/help listed the commands")
send("/model\r")
require(pump(re.compile(r"model=")), "/model reported the active model")
send("/nonesuch\r")
require(pump(re.compile(r"is not an fxr command")), "an unknown command was refused")
send("설명해줘 — a unicode prompt\r")
require(pump(re.compile(r"shell answer")), "a prompt was answered through the Gateway")
send("/quit\r")

status = None
limit = time.time() + deadline_seconds
while time.time() < limit:
    ready, _, _ = select.select([fd], [], [], 0.1)
    if ready:
        try:
            chunk = os.read(fd, 4096)
        except OSError:
            chunk = b""
        if chunk:
            captured.extend(chunk)
            continue
    finished, raw = os.waitpid(pid, os.WNOHANG)
    if finished:
        status = raw
        break
if status is None:
    os.kill(pid, 9)
    os.waitpid(pid, 0)

with open(transcript_path, "wb") as handle:
    handle.write(bytes(captured))

exit_code = os.waitstatus_to_exitcode(status) if status is not None else 1
require(exit_code == 0, f"/quit left with status {exit_code}, wanted 0")
require(b"\x1b[?1049" not in bytes(captured), "the shell never took the alternate screen")

for problem in problems:
    print(problem)
sys.exit(1 if problems else 0)
PYTHON

gateway_pid=""
gateway_dir=""

start_gateway() {
	local name="$1" script="$2"
	gateway_dir="$evidence/$name"
	mkdir -p "$gateway_dir"
	printf '%s' "$script" >"$gateway_dir/script.json"
	: >"$gateway_dir/requests.jsonl"
	rm -f "$gateway_dir/port"
	python3 "$helpers/fake_gateway.py" \
		"$gateway_dir/script.json" "$gateway_dir/port" "$gateway_dir/requests.jsonl" \
		>"$gateway_dir/gateway.log" 2>&1 &
	gateway_pid=$!
	for _ in $(seq 1 100); do
		if [ -s "$gateway_dir/port" ]; then
			break
		fi
		sleep 0.05
	done
	if [ ! -s "$gateway_dir/port" ]; then
		printf 'smoke: the fake Gateway did not start; see %s\n' "$gateway_dir/gateway.log" >&2
		exit 1
	fi
	gateway_url="http://127.0.0.1:$(cat "$gateway_dir/port")/v3/ai/language-model"
}

stop_gateway() {
	if [ -n "$gateway_pid" ]; then
		kill "$gateway_pid" 2>/dev/null || true
		wait "$gateway_pid" 2>/dev/null || true
		gateway_pid=""
	fi
}

cleanup() {
	stop_gateway
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# a clean machine for every scenario
# ---------------------------------------------------------------------------

home="$evidence/home"
workspace="$evidence/workspace"
mkdir -p "$home" "$workspace"

# Runs fxr with a controlled environment and captures both streams.
#
# `name` is the evidence prefix; the remaining arguments are fxr's. The exit
# status is left in `last_status`, and the streams in `$evidence/<name>.out`
# and `.err`.
run_fxr() {
	local name="$1"
	shift
	set +e
	(
		cd "$workspace" || exit 1
		env -u VERCEL_OIDC_TOKEN -u FXR_MODEL -u FXR_PERMISSION_MODE -u FXR_MAX_AGENT_STEPS \
			HOME="$home" \
			AI_GATEWAY_API_KEY="$FAKE_KEY" \
			FXR_GATEWAY_URL="${gateway_url:-}" \
			"$binary" "$@"
	) >"$evidence/$name.out" 2>"$evidence/$name.err"
	last_status=$?
	set -e
	printf '$ fxr %s\n  exit=%s\n' "$*" "$last_status" >>"$evidence/transcript.txt"
	cat "$evidence/$name.out" >>"$evidence/transcript.txt"
	cat "$evidence/$name.err" >>"$evidence/transcript.txt"
	printf '\n' >>"$evidence/transcript.txt"
}

printf 'fxr smoke\n  binary:   %s\n  evidence: %s\n\n' "$binary" "$evidence"
: >"$evidence/transcript.txt"

# ---------------------------------------------------------------------------
# 1. what the product says about itself, without a credential or a network
# ---------------------------------------------------------------------------

printf '1. help, version, status, doctor\n'
gateway_url=""

run_fxr help --help
expect_status "$last_status" 0 "--help exits 0"
expect_contains "$evidence/help.out" "Usage: fxr" "--help shows usage"
for deferred in " acp" " login" " upgrade" " replay"; do
	expect_absent "$evidence/help.out" "$deferred" "--help does not advertise$deferred"
done

run_fxr version --version
expect_status "$last_status" 0 "--version exits 0"

run_fxr status status --json
expect_status "$last_status" 0 "status --json exits 0"
expect_contains "$evidence/status.out" '"sandbox":"none"' "status reports no sandbox"
expect_contains "$evidence/status.out" '"permission_mode"' "status reports the permission mode"
if [ "$(wc -l <"$evidence/status.out")" -eq 1 ]; then
	pass "status --json is exactly one document"
else
	fail "status --json is exactly one document"
fi

run_fxr doctor doctor --json
expect_status "$last_status" 0 "doctor --json exits 0"
expect_contains "$evidence/doctor.out" '"name":"sessions"' "doctor checks the session store"
expect_contains "$evidence/doctor.out" '"name":"permissions"' "doctor checks the permission mode"

run_fxr bare
expect_status "$last_status" 1 "a bare fxr without a terminal exits 1"
expect_contains "$evidence/bare.err" "interactive terminal" "it says a terminal is required"

# ---------------------------------------------------------------------------
# 2. a content-only answer
# ---------------------------------------------------------------------------

printf '\n2. a content-only turn\n'
start_gateway content '[
  {"events": [
    {"type": "text-delta", "id": "a", "delta": "the answer "},
    {"type": "text-delta", "id": "a", "delta": "is content only"},
    {"type": "finish", "finishReason": {"unified": "stop", "raw": "stop"},
     "usage": {"inputTokens": {"total": 3}, "outputTokens": {"total": 5}}}
  ]}
]'

run_fxr content ask --no-save "say something"
expect_status "$last_status" 0 "ask exits 0"
expect_contains "$evidence/content.out" "the answer is content only" "the answer reached stdout"
expect_absent "$evidence/content.err" "$FAKE_KEY" "the credential stayed out of stderr"
expect_contains "$gateway_dir/requests.jsonl" '"authorization": "Bearer' "the request carried a bearer token"
if [ "$(wc -l <"$gateway_dir/requests.jsonl")" -eq 1 ]; then
	pass "a content-only turn is exactly one request"
else
	fail "a content-only turn is exactly one request"
fi
stop_gateway

# ---------------------------------------------------------------------------
# 3. read, edit, run a command, be refused a destructive one, finish
# ---------------------------------------------------------------------------

printf '\n3. a multi-step mutation turn\n'
printf 'alpha\n' >"$workspace/notes.txt"

start_gateway mutation '[
  {"events": [
    {"type": "tool-call", "toolCallId": "c1", "toolName": "read_file",
     "input": {"path": "notes.txt"}},
    {"type": "finish", "finishReason": {"unified": "tool-calls", "raw": "tool-calls"}}
  ]},
  {"events": [
    {"type": "tool-call", "toolCallId": "c2", "toolName": "edit_file",
     "input": {"path": "notes.txt", "old_string": "alpha", "new_string": "beta"}},
    {"type": "finish", "finishReason": {"unified": "tool-calls", "raw": "tool-calls"}}
  ]},
  {"events": [
    {"type": "tool-call", "toolCallId": "c3", "toolName": "terminal",
     "input": {"action": "exec", "command": "cat notes.txt"}},
    {"type": "finish", "finishReason": {"unified": "tool-calls", "raw": "tool-calls"}}
  ]},
  {"events": [
    {"type": "tool-call", "toolCallId": "c4", "toolName": "terminal",
     "input": {"action": "exec", "command": "rm -rf notes.txt"}},
    {"type": "finish", "finishReason": {"unified": "tool-calls", "raw": "tool-calls"}}
  ]},
  {"events": [
    {"type": "text-delta", "id": "z", "delta": "edited notes.txt and read it back"},
    {"type": "finish", "finishReason": {"unified": "stop", "raw": "stop"}}
  ]}
]'

run_fxr mutation ask --auto --json --no-save "edit the notes and check them"
expect_status "$last_status" 0 "a five-step turn exits 0"
expect_contains "$evidence/mutation.out" '"kind":"final"' "the turn ended with one final event"
expect_contains "$evidence/mutation.out" '"tool":"edit_file"' "the edit ran"
expect_contains "$evidence/mutation.out" '"tool":"terminal"' "the command ran"

if grep -q '^beta$' "$workspace/notes.txt"; then
	pass "the workspace file really changed"
else
	fail "the workspace file really changed"
fi
expect_contains "$gateway_dir/requests.jsonl" "beta" "the command output went back to the model"
expect_contains "$evidence/mutation.out" '"ok":false' "the destructive command was refused"
if [ -f "$workspace/notes.txt" ]; then
	pass "the refused command did not delete the file"
else
	fail "the refused command did not delete the file"
fi
if [ "$(wc -l <"$gateway_dir/requests.jsonl")" -eq 5 ]; then
	pass "five steps are five requests"
else
	fail "five steps are five requests ($(wc -l <"$gateway_dir/requests.jsonl") seen)"
fi
stop_gateway

# ---------------------------------------------------------------------------
# 4. sessions: save, list, show, resume, rebind, and refuse to save
# ---------------------------------------------------------------------------

printf '\n4. sessions, resume, rebind, and --no-save\n'
start_gateway sessions '[
  {"events": [
    {"type": "text-delta", "id": "a", "delta": "first answer"},
    {"type": "finish", "finishReason": {"unified": "stop", "raw": "stop"}}
  ]},
  {"events": [
    {"type": "text-delta", "id": "b", "delta": "second answer"},
    {"type": "finish", "finishReason": {"unified": "stop", "raw": "stop"}}
  ]},
  {"events": [
    {"type": "text-delta", "id": "c", "delta": "rebound answer"},
    {"type": "finish", "finishReason": {"unified": "stop", "raw": "stop"}}
  ]},
  {"events": [
    {"type": "text-delta", "id": "d", "delta": "unrecorded answer"},
    {"type": "finish", "finishReason": {"unified": "stop", "raw": "stop"}}
  ]}
]'

run_fxr saved ask "remember this"
expect_status "$last_status" 0 "a recorded ask exits 0"

run_fxr list sessions --json
expect_status "$last_status" 0 "sessions --json exits 0"
# Tolerant on purpose: a listing that is not JSON is a failure this script has
# to *report*, not one it should die of halfway through.
session_id="$(python3 -c '
import json,sys
try:
    document = json.load(open(sys.argv[1], encoding="utf-8"))
    print(document["sessions"][0]["id"] if document["sessions"] else "")
except Exception:
    print("")
' "$evidence/list.out" 2>/dev/null || true)"
if [ -n "$session_id" ]; then
	pass "the turn was recorded as session $session_id"
else
	fail "the turn was recorded"
fi

run_fxr detail session last --json
expect_status "$last_status" 0 "session last --json exits 0"
expect_contains "$evidence/detail.out" "remember this" "the session kept the prompt"

run_fxr resume ask --resume last "and this"
expect_status "$last_status" 0 "resume exits 0"
set +e
python3 -c '
import json,sys
try:
    lines = [json.loads(line) for line in open(sys.argv[1], encoding="utf-8")]
    body = json.loads(lines[1]["body"])
    users = [
        part["text"]
        for message in body["prompt"] if message["role"] == "user"
        for part in message["content"] if part.get("type") == "text"
    ]
except Exception:
    users = None
sys.exit(0 if users == ["remember this", "and this"] else 1)
' "$gateway_dir/requests.jsonl" 2>/dev/null
resume_carried=$?
set -e
if [ "$resume_carried" -eq 0 ]; then
	pass "the resumed turn carried the earlier one"
else
	fail "the resumed turn carried the earlier one"
fi

# A session named by id may be resumed from another workspace; that rebinding is
# a durable event rather than a silent move.
other="$evidence/other-workspace"
mkdir -p "$other"
set +e
(
	cd "$other" &&
		env -u VERCEL_OIDC_TOKEN HOME="$home" AI_GATEWAY_API_KEY="$FAKE_KEY" \
			FXR_GATEWAY_URL="$gateway_url" \
			"$binary" ask --resume-id "$session_id" "from somewhere else"
) >"$evidence/rebind.out" 2>"$evidence/rebind.err"
last_status=$?
set -e
expect_status "$last_status" 0 "a rebinding resume exits 0"
if grep -qF 'workspace_rebound' "$home/.fxr/sessions/$session_id/events.jsonl" 2>/dev/null; then
	pass "the rebinding was recorded durably"
else
	fail "the rebinding was recorded durably"
fi

before="$(find "$home/.fxr/sessions" -maxdepth 1 -mindepth 1 2>/dev/null | wc -l | tr -d ' ' || true)"
run_fxr nosave ask --no-save "do not remember this"
expect_status "$last_status" 0 "--no-save exits 0"
after="$(find "$home/.fxr/sessions" -maxdepth 1 -mindepth 1 2>/dev/null | wc -l | tr -d ' ' || true)"
if [ "$before" = "$after" ]; then
	pass "--no-save created nothing under the profile home"
else
	fail "--no-save created nothing under the profile home ($before -> $after)"
fi
stop_gateway

# ---------------------------------------------------------------------------
# 5. the interactive shell, on a real pseudoterminal
# ---------------------------------------------------------------------------

printf '\n5. the interactive shell\n'
start_gateway shell '[
  {"events": [
    {"type": "text-delta", "id": "a", "delta": "shell answer"},
    {"type": "finish", "finishReason": {"unified": "stop", "raw": "stop"}}
  ]}
]'

set +e
python3 "$helpers/pty_shell.py" \
	"$binary" "$workspace" "$evidence/shell-transcript.txt" \
	"HOME=$home" "AI_GATEWAY_API_KEY=$FAKE_KEY" "FXR_GATEWAY_URL=$gateway_url" \
	"TERM=dumb" >"$evidence/shell.out" 2>"$evidence/shell.err"
shell_status=$?
set -e
if [ "$shell_status" -eq 0 ]; then
	pass "the shell greeted, answered, refused an unknown command, and quit cleanly"
else
	fail "the shell run reported: $(tr '\n' '; ' <"$evidence/shell.out")"
fi
expect_contains "$evidence/shell-transcript.txt" "/version" "the shell listed its commands"
expect_contains "$evidence/shell-transcript.txt" "shell answer" "the shell streamed an answer"
stop_gateway

# ---------------------------------------------------------------------------
# 6. nothing leaked
# ---------------------------------------------------------------------------

printf '\n6. no credential in any captured stream\n'
# Done in python rather than with `grep -r`, because the recursive spellings
# differ between greps: `--exclude-dir` is not portable, and a `grep` that
# silently searched nothing would turn this into a check that cannot fail.
#
# It is deliberately two-sided. The fake Gateway's own request log *must*
# contain the bearer token -- that is the protocol working -- and every other
# captured stream must not. Requiring the positive half is what proves the
# negative half was actually looking.
set +e
python3 - "$evidence" "$FAKE_KEY" >"$evidence/leaks.txt" 2>&1 <<'PYTHON'
import os
import sys

root, key = sys.argv[1], sys.argv[2]
expected, leaked, scanned = [], [], 0
for directory, subdirectories, files in os.walk(root):
    if os.path.basename(directory) == "helpers":
        subdirectories[:] = []
        continue
    for name in files:
        if name == "leaks.txt":
            continue
        path = os.path.join(directory, name)
        try:
            with open(path, "rb") as handle:
                blob = handle.read()
        except OSError:
            continue
        scanned += 1
        if key.encode() not in blob:
            continue
        (expected if name == "requests.jsonl" else leaked).append(
            os.path.relpath(path, root)
        )

print(f"scanned {scanned} file(s)")
for path in leaked:
    print(f"leaked {path}")
if not expected:
    print("the Gateway request log did not contain the key, so nothing was proven")
sys.exit(1 if leaked or not expected else 0)
PYTHON
leak_status=$?
set -e
if [ "$leak_status" -eq 0 ]; then
	pass "the credential appears only in the Gateway's own request log ($(head -1 "$evidence/leaks.txt"))"
else
	fail "credential scan: $(tr '\n' '; ' <"$evidence/leaks.txt")"
fi

printf '\n%d check(s), %d failure(s)\nevidence: %s\n' "$checks" "$failures" "$evidence"
if [ "$failures" -ne 0 ]; then
	exit 1
fi
