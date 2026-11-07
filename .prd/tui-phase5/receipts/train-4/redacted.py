#!/usr/bin/env python3
"""Writes stdin to <path> through scripts/fxcap/redact.py's `Redactor.write`
(which refuses a text with a local path left in it): every sweep-derived file
of this receipt goes through it.

    ... | python3 -I -B redacted.py <path> [<root>=<name>]...

Each <root>=<name> is a known root written as its name, longest first
(the sweeps' out directories, the train's checkout); anything else under a
local root becomes `<local-path>`. After that, a fragment of a checkout's
path that a capture row cut short (`...ai/xfx/.worktrees/<name>/scripts/...`,
with no local root left at its start) becomes `<checkout>/`, and the
account's name, wherever it is left, `<user>`.
"""
import getpass
import re
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "..", "scripts", "fxcap"))
import redact  # noqa: E402

known = dict(arg.split("=", 1) for arg in sys.argv[2:])
redactor = redact.Redactor(known)
text = redactor(sys.stdin.read())
text = re.sub(r"[^\s'\"`(<]*\.worktrees/[\w.-]+/", "<checkout>/", text)
user = getpass.getuser()
if len(user) > 2:
    text = text.replace(user, "<user>")
redactor.write(sys.argv[1], text)
