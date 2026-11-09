#!/usr/bin/env python3
"""Train 11, join check 3 (train 10's shared.py, its function list this
train's): CATALOG-REFRESH x PASTE-GAP, the event loop's turn order, and
the functions both concern. The loop itself (`session`: collect_facts ->
read_burst -> settle_input -> take_ui_events -> settle_band ->
collect_facts -> commit_counted) and its parts: the reads that feed the
decoder (`read_burst`, `burst`, `Shell::route_read`, `end_delivery_at`,
`still_at`, `said_ignored`), the reconcile (`settle_input`,
`plane_names_the_hold`), the events (`take_ui_events`, `drain_ui_events`,
`Shell::apply`, `consume`), `settle_band`, the frame (`commit_counted`,
`commit_tick`, `commit_frame`, `settle_column`), the catalog's clock and
asks (`catalog_now`, `note_catalog_activity`, `want_catalog`), and the
submit SUBMIT-CARD-PREVIEW previews (`send`, `submit`, `band_landed`,
`preview_landed`). Each read on every side and at the head, its text's
sha256 (first 12 hex digits) per commit. A function the same on every side
that holds it was changed by no merge; for one that differs, the sides that
changed it from their base and whose the head's is.

    python3 -I -B shared.py <repo> <head> <side>=<commit>[:<base>]...
"""
import hashlib
import re
import subprocess
import sys

EL = "src/tui/event_loop.rs"
SH = "src/tui/shell.rs"
FUNCTIONS = [
    # the loop and its reads
    (EL, "fn session(", "\n}\n"),
    (EL, "fn collect_facts(", "\n}\n"),
    (EL, "fn read_burst(", "\n}\n"),
    (EL, "fn burst(", "\n}\n"),
    (SH, "    pub(crate) fn route_read(", "\n    }\n"),
    (SH, "    pub(crate) fn end_delivery_at(", "\n    }\n"),
    (SH, "    pub(crate) fn still_at(", "\n    }\n"),
    (SH, "    fn said_ignored(", "\n    }\n"),
    # PASTE-GAP's reconcile
    (SH, "    pub(crate) fn settle_input(", "\n    }\n"),
    (SH, "    fn plane_names_the_hold(", "\n    }\n"),
    # the events, CATALOG-REFRESH's among them
    (EL, "fn take_ui_events(", "\n}\n"),
    (EL, "fn drain_ui_events(", "\n}\n"),
    (SH, "    pub(crate) fn apply(", "\n    }\n"),
    (SH, "    fn consume(", "\n    }\n"),
    (SH, "    pub(crate) fn settle_band(", "\n    }\n"),
    # the frame
    (EL, "fn commit_counted(", "\n}\n"),
    (EL, "fn commit_tick(", "\n}\n"),
    (EL, "fn commit_frame(", "\n}\n"),
    (SH, "    pub(crate) fn settle_column(", "\n    }\n"),
    # the catalog's clock and asks
    (SH, "    pub(crate) fn catalog_now(", "\n    }\n"),
    (SH, "    fn note_catalog_activity(", "\n    }\n"),
    (SH, "    fn want_catalog(", "\n    }\n"),
    # the submit and its preview
    (SH, "    fn send(&mut self", "\n    }\n"),
    (SH, "    fn submit(&mut self)", "\n    }\n"),
    (SH, "    pub(crate) fn band_landed(", "\n    }\n"),
    (SH, "    pub(crate) fn preview_landed(", "\n    }\n"),
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
