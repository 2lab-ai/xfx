#!/usr/bin/env bash
# The capture harness's own tests (scripts/fxcap/test_capture.py): every verb
# capture.sh has for the 0.0.16 delta (resize, signal and jobctl, rehomefile,
# clock, usage, models2, modelsdown and modelsup, modelsdelay, modelspad and modelsstall, mcphttp,
# sockets, held and release, until, absent and expect, the launch wait and nodraw, the placeholders,
# each refusal), run end to end over a stub binary -- a link to /bin/bash or
# /bin/cat, never fx or xfx -- on tmux servers of their own; and fakegw.ts,
# mcphttp.ts and the p5-mcp stdio fixture, asked directly on 127.0.0.1.
# Needs tmux, bun and curl, and fails without them: unlike test-sweep.sh it
# runs tmux, so it is not part of that script.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
for script in "$here"/*.sh; do bash -n "$script"; done
exec python3 -I "$here/test_capture.py" "$@"
