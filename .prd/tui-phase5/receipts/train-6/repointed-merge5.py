#!/usr/bin/env python3
"""Merge 5's (ARGROW-CACHE's) re-pointed refs, each checked against the code it
named at the train's head before the merge (99eb8a13), with the lines either
side; the grown range (`cluster_cells`, ARGROW-CACHE's ASCII fast path inside
it) compared with the inserted lines taken out.

    python3 -I -B repointed-merge5.py <repo> <head>
"""
import subprocess
import sys

FRAME = "99eb8a13"
ROWS = [
    # who cites it, path, was, now, n, ctx
    ("ledger ctrl-o-drawn-wider-named, answer-modifier-sequence-wider (`fn drawn_wider_named`)", "src/tui/full_transcript.rs", 2743, 3005, 1, 2),
    ("ledger answer-drawn-wider-named, answer-label-and-modifier-named (the doc above `fn shown_event`)", "src/tui/full_transcript.rs", 2794, 3056, 1, 2),
    ("ledger filler-modifier-count (`fn upstream_cells`)", "src/tui/wrap.rs", 656, 726, 1, 2),
    ("event_loop.rs, the wrap into 40 + 40 and 10 (`wrap`'s word state)", "src/tui/wrap.rs", 232, 293, 9, 2),
]
# who cites it, path, was (first, last), now (first, last), the head's inserted lines (first, last)
GROWN = [
    ("ledger ri-run-pairing (`fn cluster_cells` to its `upstream_cells` arm)", "src/tui/wrap.rs", (612, 623), (673, 693), (674, 682)),
]


def show(repo, commit, path):
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], check=True,
                          capture_output=True, text=True).stdout.split("\n")


def main():
    repo, head = sys.argv[1], sys.argv[2]
    ok = total = 0
    for who, path, was, now, n, ctx in ROWS:
        a, b = show(repo, FRAME, path), show(repo, head, path)
        same = a[was - 1 - ctx:was - 1 + n + ctx] == b[now - 1 - ctx:now - 1 + n + ctx]
        ok += same; total += 1
        span = lambda x: f"{x}" + (f"-{x + n - 1}" if n > 1 else "")
        print(f"{'SAME' if same else 'DIFFERENT'}  {path}:{span(was)}@{FRAME} -> :{span(now)} (+-{ctx} lines)  [{who}]\n    first: {b[now - 1].strip()[:110]}")
    for who, path, (w1, w2), (n1, n2), (i1, i2) in GROWN:
        a, b = show(repo, FRAME, path), show(repo, head, path)
        kept = b[n1 - 1 - 2:i1 - 1] + b[i2:n2 + 2]
        same = a[w1 - 1 - 2:w2 + 2] == kept
        ok += same; total += 1
        print(f"{'SAME' if same else 'DIFFERENT'}  {path}:{w1}-{w2}@{FRAME} -> :{n1}-{n2}, its lines {i1}-{i2} taken out (+-2 lines)  [{who}]\n"
              f"    inserted: {' / '.join(x.strip() for x in b[i1 - 1:i2] if x.strip())[:200]}")
    print(f"# {ok} of {total} the same code")


main()
