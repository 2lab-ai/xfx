#!/usr/bin/env python3
"""Which functions on this unit's path each side of the join changed.

    python3 -I fns.py <repo>     (from the merge, before or after its commit)

For each function, its body is taken at the merge base (e2fc7e94), at this
branch before the join (ef595df2), at train 16 (8be4693b) and in the
working tree, and the line says which of them differ: `ours` = this branch
changed it since the base, `theirs` = the trains did, and `tree` = what the
merge left, which must be the side that changed it (or either, when neither
did). A function both sides changed is the merge's to read by hand.
Prints no path.
"""
import re
import subprocess
import sys

repo = sys.argv[1]
REVS = {"base": "e2fc7e94", "ours": "ef595df2", "theirs": "8be4693b"}
FNS = {
    "src/tui/event_loop.rs": ["session", "loop_turn", "commit_or_hold", "take_ui_events",
                              "drain_ui_events", "read_burst", "collect_facts", "commit_tick"],
    "src/tui/shell.rs": ["route_read", "settle_input", "settle_band", "consume", "consume_inputs",
                         "keyed", "take_status_moved_by_input", "turn_start_owes_its_frame",
                         "status_activity", "waiting_prompts", "apply", "end_delivery",
                         "end_delivery_at", "still_at"],
    "src/tui/input.rs": ["feed", "flush"],
    # The runtime's side of a turn: the start, the answer and the end the
    # drain reads, and the place given back (added after the reviewer's
    # question; this unit's comments name neither, but its fix rests on
    # what they send).
    "src/tui/worker.rs": ["turn_loop", "run_turn"],
}


def body(text, name):
    found = [m.start() for m in re.finditer(r"\n(?:    )*(?:pub(?:\([a-z]+\))? )?(?:async )?fn " + name + r"\b", text)]
    # The first definition outside `mod tests`, the file's code.
    tests = text.find("\nmod tests {")
    found = [at for at in found if tests < 0 or at < tests] or found[:1]
    if not found:
        return None
    at = text.index("{", found[0])
    depth = 0
    for i in range(at, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[found[0]:i + 1]
    return None


def show(rev, path):
    if rev is None:
        return open(f"{repo}/{path}").read()
    return subprocess.run(["git", "-C", repo, "show", f"{rev}:{path}"],
                          capture_output=True, text=True, check=True).stdout


both = 0
for path, names in FNS.items():
    texts = {k: show(v, path) for k, v in REVS.items()}
    texts["tree"] = show(None, path)
    for name in names:
        b = {k: body(t, name) for k, t in texts.items()}
        if b["base"] is None and b["ours"] is None and b["theirs"] is None:
            print(f"{path} {name}: not found")
            continue
        ours = b["ours"] != b["base"]
        theirs = b["theirs"] != b["base"]
        if ours and theirs:
            tree = "by hand"
            both += 1
        elif ours:
            tree = "= ours" if b["tree"] == b["ours"] else "DIFFERS from ours"
        elif theirs:
            tree = "= theirs" if b["tree"] == b["theirs"] else "DIFFERS from theirs"
        else:
            tree = "= base" if b["tree"] == b["base"] else "DIFFERS from base"
        changed = "+".join(s for s, c in (("ours", ours), ("theirs", theirs)) if c) or "neither"
        print(f"{path} {name}: changed by {changed}; tree {tree}")
print(f"changed by both: {both}")
