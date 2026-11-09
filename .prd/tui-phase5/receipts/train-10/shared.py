#!/usr/bin/env python3
"""Train 10, join check 2 (train 9's shared.py, its function list
widened): where COMPACT-NOTICE-LATE's compaction ending meets
HISTORY-NOTICE-STOP's notice marking (on main since train 9). The notice's
path -- the turn that finds, sends and marks it (`one_turn`), the
conversation's ledger (`Conversation::unsaid`, `said`), the screen that takes
it (the `UiEvent::Notice` arm, `announce`, `mark`, `abandon_answer`),
`bridge::send_ui` -- and the compaction's: the stop's drop
(`Queue::abandon`, `tell_dropped`), the reset that tells it first
(`reset_conversation`, `drop_conversation`), `run_compact`, `turn_loop`,
`send_terminal`, `try_send_terminal`, the band's `compaction_update`,
`settle_band`, `stop_compaction`, `submit`, the reset's drain (`drained`,
which drops a `Notice` and a `Compaction` alike), and `Compaction::answer`,
`settle`. Each read on every side and at the head, its text's sha256 (first
12 hex digits) per commit. A function the same on every side that holds it
was changed by no merge; for one that differs, the sides that changed it
from main's and whose the head's is.

    python3 -I -B shared.py <repo> <head> <side>=<commit>...
"""
import hashlib
import re
import subprocess
import sys

FUNCTIONS = [
    # HISTORY-NOTICE-STOP's notice
    ("src/tui/worker.rs", "async fn one_turn(", "\n}\n"),
    ("src/interactive.rs", "    pub(crate) fn unsaid(", "\n    }\n"),
    ("src/interactive.rs", "    pub(crate) fn said(", "\n    }\n"),
    ("src/tui/shell.rs", "            UiEvent::Notice(notice) => self.announce(notice),", "\n"),
    ("src/tui/shell.rs", "    fn announce(", "\n    }\n"),
    ("src/tui/shell.rs", "    fn mark(&mut self", "\n    }\n"),
    ("src/tui/shell.rs", "    fn abandon_answer(", "\n    }\n"),
    ("src/tui/bridge.rs", "pub(crate) async fn send_ui(", "\n}\n"),
    # COMPACT-NOTICE-LATE's ending
    ("src/tui/worker.rs", "    fn abandon(&mut self", "\n    }\n"),
    ("src/tui/worker.rs", "    async fn tell_dropped(", "\n    }\n"),
    ("src/tui/worker.rs", "async fn reset_conversation(", "\n}\n"),
    ("src/tui/worker.rs", "async fn drop_conversation(", "\n}\n"),
    ("src/tui/worker.rs", "async fn run_compact(", "\n}\n"),
    ("src/tui/worker.rs", "async fn turn_loop(", "\n}\n"),
    ("src/tui/bridge.rs", "pub(crate) async fn send_terminal(", "\n}\n"),
    ("src/tui/bridge.rs", "pub(crate) fn try_send_terminal(", "\n}\n"),
    ("src/tui/shell.rs", "    fn compaction_update(", "\n    }\n"),
    ("src/tui/shell.rs", "    pub(crate) fn settle_band(", "\n    }\n"),
    ("src/tui/shell.rs", "    fn stop_compaction(", "\n    }\n"),
    ("src/tui/shell.rs", "    fn submit(&mut self)", "\n    }\n"),
    ("src/tui/shell.rs", "    fn drained(&mut self", "\n    }\n"),
    ("src/tui/compaction.rs", "    pub(crate) fn answer(", "\n    }\n"),
    ("src/tui/compaction.rs", "    pub(crate) fn settle(", "\n    }\n"),
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


NUMS = re.compile(r":\d+(?:-\d+)?")


def digest(text, masked=False):
    if text is None:
        return "-"
    return hashlib.sha256((NUMS.sub(":N", text) if masked else text).encode()).hexdigest()[:12]


def main():
    # <name>=<tip>[:<base>]: a unit's base given, "changed by" means its tip's
    # text differs from its base's (reference numbers masked: a re-point is
    # no change); main is given without one.
    repo, head = sys.argv[1], sys.argv[2]
    sides = []
    for arg in sys.argv[3:]:
        name, commits = arg.split("=", 1)
        tip, _, base = commits.partition(":")
        sides.append((name, tip, base or None))
    same = 0
    for path, start, end in FUNCTIONS:
        cache = {}

        def text(commit):
            if commit not in cache:
                cache[commit] = body(show(repo, commit, path), start, end)
            return cache[commit]

        raw = {name: digest(text(tip)) for name, tip, _ in sides}
        raw["head"] = digest(text(head))
        masked = {name: digest(text(tip), True) for name, tip, _ in sides}
        masked["head"] = digest(text(head), True)
        changed = [name for name, tip, base in sides if base and digest(text(tip), True) != digest(text(base), True)]
        held = {d for d in masked.values() if d != "-"}
        theirs = [name for name, _, _ in sides if masked[name] == masked["head"]]
        if len(held) == 1:
            verdict = "the same on every side that holds it"
            same += 1
        else:
            verdict = (f"changed by {', '.join(changed) or 'no unit (main\'s is newer than an older base\'s)'}; "
                       f"the head's is {', '.join(theirs) or 'NO SIDE'}'s")
        if raw["head"] not in [raw[n] for n, _, _ in sides] and theirs:
            verdict += " (the same once reference numbers are masked: the re-point commit's)"
        label = re.sub(r"\s+", " ", start.strip())[:60]
        print(f"{path} `{label}`: {verdict}")
        print("    " + " | ".join(f"{n} {d}" for n, d in raw.items()))
    print(f"# {len(FUNCTIONS)} functions; {same} the same on every side that holds them")


main()
