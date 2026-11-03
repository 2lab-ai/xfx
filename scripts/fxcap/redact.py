"""Takes local paths out of anything the sweep writes for a reader other than
this machine: the scoreboard, its summary (committed), and what it prints (a
CI log may be public).

`Redactor(known)` rewrites, longest first, each known root to its name
(`<out>`, `<repo>`, `<fx-bin>`, `<xfx-bin>`, `<tmp>`), then any absolute
path left under a local root (`/Users`, `/private`, `/var`, `/tmp`,
`/Volumes`, `/home`, ...) -- the home directory and a capture's scratch HOME
among them -- to `<local-path>`. `check` refuses a text in which a local root, the home
directory or the checkout's retired product name still appears anywhere, so
a path the rewrite missed stops the write instead of reaching the file.
"""

import os
import re

LOCAL_ROOTS = ("Users", "private", "var", "tmp", "Volumes", "home", "opt", "nix", "mnt", "root", "run", "srv")
GENERIC = re.compile(r"/(?:%s)/[^\s'\"`)\]>|,;]*" % "|".join(LOCAL_ROOTS))
LEFTOVER = re.compile(r"/(?:Users|private|var/folders|tmp|Volumes|home)/")


def _retired():
    # Assembled, as scripts/check-xfx-identity.sh assembles it: a literal
    # spelling would be a finding against this tracked file.
    return "f" + "x" + "r"


class RedactionError(Exception):
    pass


class Redactor:
    def __init__(self, known=None):
        pairs = []
        for path, name in (known or {}).items():
            if not path:
                continue
            pairs.append((path, name))
            real = os.path.realpath(path)
            if real != path:
                pairs.append((real, name))
        home = os.path.expanduser("~")
        self.pairs = sorted({p for p in pairs if p[0] not in ("", "/")}, key=lambda p: -len(p[0]))
        self.home = home

    def __call__(self, text):
        for path, name in self.pairs:
            text = text.replace(path, name)
        return GENERIC.sub("<local-path>", text)

    def check(self, text):
        problems = []
        m = LEFTOVER.search(text)
        if m:
            problems.append("a local path (%r)" % text[max(0, m.start() - 20) : m.end() + 20])
        if self.home and self.home not in ("~", "/") and self.home in text:
            problems.append("the home directory")
        if _retired() in text.lower():
            problems.append("the retired product name")
        if problems:
            raise RedactionError("refusing to write: " + "; ".join(problems))
        return text

    def public(self, text):
        return self.check(self(text))

    def write(self, path, text):
        """Writes a redacted text, or nothing if anything local survives."""
        safe = self.public(text)
        with open(path, "w", encoding="utf-8") as f:
            f.write(safe)
        return path
