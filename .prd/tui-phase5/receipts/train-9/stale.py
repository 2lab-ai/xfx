#!/usr/bin/env python3
"""The references the widened scans flag at train 9's head that this train
did not move -- each naming other code on `main` (ccced1f5) already, or
rewritten under by a unit before this train -- shown against `main`: the
citing line at the head, and the line or lines the reference names in
`main`'s copy of the cited file. Train 8's stale.py with `main` moved to
ccced1f5 and this train's two.

- `docs/superpowers/plans/2026-08-24-provider-frame.md`'s
  `tests/tool_loop.rs:1266,1280`: a historical plan's "fake-provider
  completions" (a `Completion { .. }` literal), which on `main` names a
  `ToolLimits` field instead; QUESTIONS-20's eight lines in
  `the_advertisement_carries_one_closed_schema_per_tool_in_registry_order`
  move it to `:1274`. Train 8 left the plans under `docs/superpowers/plans/`
  for REF-LINT: not re-pointed here either.
- `event_loop.rs`'s `frame.rs:1615-1628`: train 8's stale.txt; flagged again
  because both units' frames are older than GRID-ROW-COST, which rewrote the
  lines (`CHANGED`); `main` and the head have the same lines.

    python3 -I -B stale.py <repo> <head>
"""
import subprocess
import sys

MAIN = "ccced1f5"
REFS = [
    # citing file, the text that cites, cited path, first, last, what the citation says the code is
    ("docs/superpowers/plans/2026-08-24-provider-frame.md", "tests/tool_loop.rs:1266", "tests/tool_loop.rs",
     1266, 1266, "a fake-provider `Completion { .. }` literal (the plan of 2026-08-24)"),
    ("src/tui/event_loop.rs", "frame.rs:1615-1628", "src/tui/frame.rs", 1615, 1628,
     "the escape `place()` writes once per document row (train 8's stale.txt)"),
]


def git(repo, *args):
    return subprocess.run(["git", "-C", repo, *args], check=True, capture_output=True, text=True).stdout


def main():
    repo, head = sys.argv[1], sys.argv[2]
    for citing, text, path, a, z, says in REFS:
        hit = [l for l in git(repo, "grep", "-n", "-F", text, head, "--", citing).splitlines()]
        cited = git(repo, "show", f"{MAIN}:{path}").splitlines()[a - 1:z]
        print(f"{citing} cites {text} for {says}")
        for line in hit:
            body = line.split(":", 2)[2]
            at = body.find(text)
            print(f"  at the head: ...{body[max(0, at - 60):at + 60].strip()}...")
        for k, line in enumerate(cited):
            print(f"  main {path}:{a + k}: {line.strip()[:110]}")
    print(f"{len(REFS)} references, each naming other code on main or rewritten under before this train")


main()
