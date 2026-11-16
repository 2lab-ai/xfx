#!/usr/bin/env python3
"""Train 16: train 15's repoint.py, its HAND, TEXT and EXTRA tables this
train's (train 15's were empty), its two LEAVE entries kept -- the two refs
train 14 left WRONG on purpose, WRONG at this train's base still. Nothing
else changed.

Train 15: train 14's repoint.py, its HAND, TEXT and EXTRA tables this
train's (train 14's were applied there), its two LEAVE entries kept -- the
two refs train 14 left WRONG on purpose, WRONG on main still. Nothing else
changed.

Train 14 (train 13's repoint.py and its join's, the plan derived instead
of listed): every reference the content check reads WRONG at <head>,
re-pointed, numbers only.

The input is content.sh's output at <head> (`--content`), and at main and
each unit's head (`--side <name>=<content file>`), which say where each
WRONG ref came from:
  merges   WRONG at the head and at no side: the merges moved it (a ref one
           side wrote, into a file the other side's lines changed);
  <side>   WRONG on that side already (the same citing file, the same frame
           commit and the same frame number): older than this train.
A ref's frame is the commit that wrote its numbers (refs_content.py's
`frame`); its text is the frame's cited lines.

The new number (`auto`): the frame's cited lines **and two either side**
stand at exactly one place of the head's cited file (train 13's check,
repointed.py's); or (`lines`) the cited lines alone stand at exactly one
place, a neighbour having changed. Otherwise the ref is in HAND, the table
below, each entry
read by hand at the head, with the text it is meant to name; a WRONG ref in
neither stops --derive.

    python3 -I -B repoint.py <repo> <head> --content <file> [--side <name>=<file>]... --derive
    python3 -I -B repoint.py <repo> <head> --content <file> [--side <name>=<file>]... --apply

--apply writes each new number into the worktree's citing line (the ref's
own token, found again by refs_content.py's `refs_of`), numbers only, and
prints what it wrote. repointed.py checks the result.
"""
import os
import re
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
import refs_content as rc  # noqa: E402

LINE = re.compile(r"^WRONG (\S+?):(\d+) (\S+) \((\w+); written by ([0-9a-f]{8}) as :(\d+)(?:-(\d+))?\): (.*)$")

# Read by hand at the head: (citing file, the ref as written, its kind) ->
# (new number or range, the text the new number names, why). Each is a ref
# whose frame's text and two either side stand at no single place of the
# head -- rewritten code, a line its frame already got wrong, or a text
# that stands twice -- or one train 13 named wrong whose frame got it wrong.
HAND = {}
# WRONG refs left as they are, each with why (not a number to move).
LEAVE = {}
# Refs the content check reads SAME but that name the wrong line (their
# frame already did, refs_content.py's blind spot): (citing file, the line's
# text before the ref, the ref as written) -> (new number, text, why).
EXTRA = {}
# Whole spans of a citing line rewritten (a ref with a list of numbers after
# it, or one pinned to the commit its text stands in): (citing file, old) ->
# (new, why). Each must stand once in its file.
TEXT = {}


def hand(citing, spelled, kind, new, text, why):
    HAND[(citing, spelled, kind)] = (new, text, why)


L = "scripts/fxcap/parity-ledger.toml"
# Train 16: the tables are this train's; train 15's were empty. EXTRA holds
# the second re-point's four (the fourth's text corrected after 15c9744b, its
# number as applied: the frame's :6418 and the head's :6423 are both `let mut
# repaint = false;`, which extra_check.py reads): refs the content check reads SAME that refs.py
# (one) and refs_bare.py (three) read MOVED at the first re-point, outside
# the historical plans, each read by hand at the head.
EXTRA[("docs/parity.md", "as xfx's `", "src/gateway/mod.rs:1005-1006")] = (
    "1009-1010", "pub(crate) fn is_retryable_status(status: u16) -> bool { / matches!(status, 429 | 500 | 502 | 503 | 504)",
    "merge 3 (TEST-ENV-ISOLATION) put four lines above it; the content check reads it SAME because the row's last "
    "writer is the join commit, whose tree has it moved already (its blind spot); refs.py reads it MOVED against the base")
EXTRA[("scripts/fxcap/capfile.py", "# transcript at ", ":15785-15787")] = (
    "15860-15862", "if self.owner != ScreenOwner::Primary { / return None; / }",
    "the bare continuation of the `src/tui/shell.rs:15842-15867` the first re-point moved; refs_bare.py MOVED")
EXTRA[("src/tui/shell.rs", "// `fetch_sub`, `", ":3535")] = (
    "3594", "queue.outstanding.fetch_sub(1, Ordering::Release);",
    "`turn_loop`'s `fetch_sub`, the comment says; :3535 was never it -- train 11's 6ae87955 wrote :3535 where "
    "`bridge::send_terminal(events, answer).await;` stood (that frame's :3561 is the fetch_sub), so the content check, "
    "reading the frame's text, calls it SAME; refs_bare.py reads it MOVED to that text's :3568, which would keep it "
    "wrong. Re-pointed to the line it names")
EXTRA[("src/tui/shell.rs", "        // `", ":6418")] = (
    "6423", "let mut repaint = false;",
    "the bare continuation of the `shell.rs:6212` the first re-point moved to :6217; refs_bare.py MOVED. The pair "
    "is TRAIN14-FOLLOWUPS' (it names no too-small assertion): its numbers kept on their frame's text, as the first's")
LEAVE[("src/tools/question.rs", "Cargo.toml:42", "path")] = (
    "the comment says the crate does not enable serde_json's `preserve_order`, and Cargo.toml enables it: "
    "the claim is out of date, and the number is wrong too (:42 is `\"http2\",`); queued as TRAIN14-FOLLOWUPS "
    "with the comment's rewrite, not a re-point")
LEAVE[("docs/parity.md", "src/mcp/transport.rs:1318-1320", "path")] = (
    "pinned: the row writes it `58b463c8:src/mcp/transport.rs:1318-1320`, the commit's file, whose 1318-1320 are "
    "`is_answer` (an answer is the frame with no method and its id); the check reads the path against the head, "
    "where TEST-ENV-ISOLATION moved those lines. The first re-point (c637ed4c) moved it with the rest; the third "
    "(train 16's) puts it back")
LEAVE[("tests/session_compat.rs", "src/session/event.rs:98-192", "path")] = (
    "it names commit 14ad04202fe5's file (`14ad04202fe5 src/session/event.rs:98-192`), whose 98-192 are "
    "`SessionEvent` as it says; the check reads it against the head")


def parse(path):
    out = []
    citing = None
    for line in open(path, encoding="utf-8"):
        m = LINE.match(line.rstrip("\n"))
        if m:
            out.append({"citing": m.group(1), "n": int(m.group(2)), "spelled": m.group(3), "kind": m.group(4),
                        "frame": m.group(5), "f0": int(m.group(6)), "l0": int(m.group(7)) if m.group(7) else None,
                        "rest": m.group(8)})
    return out


def key(w):
    return (w["citing"], w["frame"], w["f0"], w["l0"], w["kind"])


def unique_window(frame, path, f0, l0, head, ctx=rc.CTX):
    """Where the frame's cited lines and `ctx` either side stand at the
    head, if at exactly one place: (first, last) or None."""
    old = rc.show(frame, path)
    new = rc.show(head, path)
    if old is None or new is None:
        return None
    last = l0 or f0
    a, b = max(f0 - 1 - ctx, 0), min(last + ctx, len(old))
    win = old[a:b]
    lead = (f0 - 1) - a
    hits = [j for j in range(len(new) - len(win) + 1) if new[j:j + len(win)] == win]
    if len(hits) != 1:
        return None
    first = hits[0] + lead + 1
    return first, first + (last - f0)


def plan(repo, head, content, sides):
    rc.setup(repo, head, [])
    side_keys = {name: {key(w) for w in parse(f)} for name, f in sides}
    out, missing = [], []
    for w in parse(content):
        origin = next((name for name, keys in side_keys.items() if key(w) in keys), "merges")
        line = rc.show(head, w["citing"])[w["n"] - 1]
        ref = next(r for r in rc.refs_of(line) if line[r[0]:r[1]] == w["spelled"] and r[5] == w["kind"])
        path = ref[2]
        if (w["citing"], w["spelled"], w["kind"]) in LEAVE:
            out.append((w, origin, path, None, "left", LEAVE[(w["citing"], w["spelled"], w["kind"])]))
            continue
        if any(c == w["citing"] and o.startswith("(" + w["spelled"]) or (c == w["citing"] and w["spelled"] in o)
               for c, o in TEXT):
            new, why = next(v for (c, o), v in TEXT.items() if c == w["citing"] and w["spelled"] in o)
            out.append((w, origin, path, None, "text", f"{why}; the span becomes {new!r}"))
            continue
        h = HAND.get((w["citing"], w["spelled"], w["kind"]))
        if h:
            new, text, why = h
            out.append((w, origin, path, new, "hand", f"{why}; the head's :{new} holds {text!r}"))
            continue
        win = unique_window(w["frame"], path, w["f0"], w["l0"], head)
        how, why = "auto", "the frame's lines and two either side stand there alone"
        if win is None:
            win = unique_window(w["frame"], path, w["f0"], w["l0"], head, ctx=0)
            how, why = "lines", "the frame's lines stand there alone (a neighbour changed)"
        if win is None:
            missing.append((w, origin))
            continue
        new = f"{win[0]}" + (f"-{win[1]}" if w["l0"] else "")
        out.append((w, origin, path, new, how, why))
    return out, missing


def numbers(spelled, new):
    return re.sub(r":\d+(?:-\d+)?$", ":" + new, spelled)


def main():
    args = sys.argv[1:]
    repo, head = args[0], args[1]
    content = args[args.index("--content") + 1]
    sides = [tuple(args[i + 1].split("=", 1)) for i, a in enumerate(args) if a == "--side"]
    out, missing = plan(repo, head, content, sides)
    for w, origin in missing:
        print(f"NO PLAN {origin:9} {w['citing']}:{w['n']} {w['spelled']} ({w['kind']}; frame {w['frame']} :{w['f0']}): {w['rest'][:100]}")
    if missing:
        sys.exit(f"{len(missing)} WRONG refs with no plan -- read by hand")
    by = {}
    hows = {}
    for w, origin, path, new, how, why in out:
        by[origin] = by.get(origin, 0) + 1
        hows[how] = hows.get(how, 0) + 1
        to = numbers(w["spelled"], new) if new else "(as it is)" if how == "left" else "(the span below)"
        print(f"{how:5} {origin:19} {w['citing']}:{w['n']} {w['spelled']} -> {to} "
              f"(cites {path}; frame {w['frame']} :{w['f0']}{'-' + str(w['l0']) if w['l0'] else ''}): {why}")
    for (c, before, spelled), (new, text, why) in EXTRA.items():
        hows["extra"] = hows.get("extra", 0) + 1
        print(f"extra {'main':19} {c} {spelled} -> {numbers(spelled, new)} (read SAME; the head's :{new} holds {text!r}): {why}")
    print(f"# {len(out)} WRONG refs, by where they came from: " + "; ".join(f"{k} {v}" for k, v in sorted(by.items())))
    print(f"# by how: " + "; ".join(f"{k} {v}" for k, v in sorted(hows.items())) + " (extra: read SAME, re-pointed by hand)")
    if "--apply" in args:
        edits = {}
        for w, origin, path, new, how, why in out:
            if new is None:
                continue
            edits.setdefault(w["citing"], {}).setdefault(w["n"], []).append((w, new))
        for citing, by_line in edits.items():
            lines = open(os.path.join(repo, citing), encoding="utf-8").read().split("\n")
            for n, items in by_line.items():
                line = lines[n - 1]
                found = rc.refs_of(line)
                taken, spans = set(), []
                for w, new in items:  # each WRONG ref to its own token, in order of occurrence
                    i = next(i for i, r in enumerate(found) if i not in taken
                             and line[r[0]:r[1]] == w["spelled"] and r[5] == w["kind"])
                    taken.add(i)
                    spans.append((found[i][0], found[i][1], numbers(w["spelled"], new)))
                for a, b, text in sorted(spans, reverse=True):
                    line = line[:a] + text + line[b:]
                lines[n - 1] = line
            open(os.path.join(repo, citing), "w", encoding="utf-8").write("\n".join(lines))
        for (c, before, spelled), (new, text, why) in EXTRA.items():
            body = open(os.path.join(repo, c), encoding="utf-8").read()
            assert body.count(before + spelled) == 1, (c, before + spelled)
            open(os.path.join(repo, c), "w", encoding="utf-8").write(body.replace(before + spelled, before + numbers(spelled, new)))
        for (c, old_span), (new_span, why) in TEXT.items():
            body = open(os.path.join(repo, c), encoding="utf-8").read()
            assert body.count(old_span) == 1, (c, old_span)
            open(os.path.join(repo, c), "w", encoding="utf-8").write(body.replace(old_span, new_span))
        n = sum(1 for o in out if o[3] is not None)
        print(f"# applied {n} numbers in {len(edits)} files, {len(EXTRA)} by hand that read SAME, {len(TEXT)} spans")


if __name__ == "__main__":
    main()
