#!/usr/bin/env python3
"""Train 13 (train 12's redacted.py, unchanged but for this line): writes stdin to <path> through scripts/fxcap/redact.py's `Redactor.write`
(which refuses a text with a local path left in it): every sweep-derived file
of this receipt goes through it.

    ... | python3 -I -B redacted.py <path> [<root>=<name>]...

Each <root>=<name> is a known root written as its name, longest first
(the sweeps' out directories, the train's checkout); anything else under a
local root becomes `<local-path>`. After that, a fragment of a checkout's
path that a capture row cut short (`...ai/xfx/.worktrees/<name>/scripts/...`,
with no local root left at its start) becomes `<checkout>/`, as does any
other path cut short before `scripts/fxcap/`, and the
account's name, wherever it is left, `<user>`.

Train 12, round 2 (the tail rule): a local root a capture row wrapped leaves
its head on one row, which the redactor writes `<local-path>`, and its tail
on the next, which no root starts. So, last, any token (a run of characters
other than white space, quotes, backquotes and `(<>`) that holds a run
directory's name -- `fxsw.` and six name characters, or `fxcap.` and six
other than the masked `XXXXXX` -- or the harness root's uid part
(`claude-<digits>`, or a token that starts with `-<digits>/`, that root's
tail cut after its `claude`) becomes `<local-path>` whole. Each part below
is spelt with a bracket class, so this file does not match the scans' own
shapes.
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
# A checkout's path cut short anywhere else (a scratch clone's, say), up to
# the harness directory it names -- a whole token's start; train 7: not a
# diff's own `a/` or `b/` (a remerge-diff's file names).
text = re.sub(r"(?<![^\s'\"`(<>])(?![ab]/scripts/fxcap/)[^\s'\"`(<>]+/(?=scripts/fxcap/)", "<checkout>/", text)
TAIL = re.compile(r"[^\s'\"`(<>]*(?:fxs[w]\.[A-Za-z0-9_]{6}|fxca[p]\.(?!XXXXXX)[A-Za-z0-9]{6}|claud[e]-[0-9]+)[^\s'\"`)<>]*"
                  r"|(?<![^\s'\"`(<>])-[0-9]+/[^\s'\"`)<>]*")
text = TAIL.sub("<local-path>", text)
user = getpass.getuser()
if len(user) > 2:
    text = text.replace(user, "<user>")
redactor.write(sys.argv[1], text)
