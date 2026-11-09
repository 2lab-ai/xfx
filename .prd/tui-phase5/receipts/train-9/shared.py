#!/usr/bin/env python3
"""Train 9, join check 7: where HISTORY-NOTICE-STOP's notice marking meets
the other units' code. Each function on the notice's path -- the turn that
finds, sends and marks the notice (`one_turn`), the conversation's ledger
(`Conversation::unsaid`, `said`), and the screen that takes it (the
`UiEvent::Notice` arm, `announce`, `mark`, `abandon_answer`) -- read on
every side and at the head, its text's sha256 (first 12 hex digits) per
commit. A function the same on every side that holds it was changed by
no merge; for one that differs, the sides that changed it from main's and
whose the head's is.

    python3 -I -B shared.py <repo> <head> <side>=<commit>...
"""
import hashlib
import re
import subprocess
import sys

FUNCTIONS = [
    ("src/tui/worker.rs", "async fn one_turn(", "\n}\n"),
    ("src/interactive.rs", "    pub(crate) fn unsaid(", "\n    }\n"),
    ("src/interactive.rs", "    pub(crate) fn said(", "\n    }\n"),
    ("src/tui/shell.rs", "            UiEvent::Notice(notice) => self.announce(notice),", "\n"),
    ("src/tui/shell.rs", "    fn announce(", "\n    }\n"),
    ("src/tui/shell.rs", "    fn mark(&mut self", "\n    }\n"),
    ("src/tui/shell.rs", "    fn abandon_answer(", "\n    }\n"),
    ("src/tui/bridge.rs", "pub(crate) async fn send_ui(", "\n}\n"),
]


def show(repo, commit, path):
    r = subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], capture_output=True, text=True)
    return r.stdout if r.returncode == 0 else ""


def body(text, start, end):
    if start not in text:
        return None
    text = text[: text.find("#[cfg(test)]\nmod tests {")] if "#[cfg(test)]\nmod tests {" in text else text
    i = text.index(start)
    return text[i: text.index(end, i + len(start)) + len(end)]


def main():
    repo, head, sides = sys.argv[1], sys.argv[2], [a.split("=", 1) for a in sys.argv[3:]]
    commits = sides + [("head", head)]
    same = 0
    for path, start, end in FUNCTIONS:
        digests = {}
        for name, commit in commits:
            b = body(show(repo, commit, path), start, end)
            digests[name] = hashlib.sha256(b.encode()).hexdigest()[:12] if b is not None else "-"
        held = {d for d in digests.values() if d != "-"}
        if len(held) == 1:
            verdict = "the same on every side that holds it"
        else:
            main = digests[commits[0][0]]
            changed = [n for n, _ in sides if digests[n] not in (main, "-")]
            ats = [n for n, _ in sides if digests[n] == digests["head"]]
            verdict = (f"changed by {', '.join(changed)}; every other side holds {commits[0][0]}'s; "
                       f"the head's is {', '.join(ats)}'s")
        same += len(held) == 1
        label = re.sub(r"\s+", " ", start.strip())[:60]
        print(f"{path} `{label}`: {verdict}")
        print("    " + " | ".join(f"{n} {d}" for n, d in digests.items()))
    print(f"# {len(FUNCTIONS)} functions; {same} the same on every side that holds them")


main()
