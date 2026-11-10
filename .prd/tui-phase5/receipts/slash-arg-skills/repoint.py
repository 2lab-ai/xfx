#!/usr/bin/env python3
"""SLASH-ARG-SKILLS' re-points: every ledger ref into a file this unit moved
that train 13's refs_content.py calls WRONG at the code head, set to the one
line where the text it was written about stands now. Numbers only.
(SLASH-LEADING-WS' repoint.py, with this unit's files.)

The ledger ref refs_content.py already called WRONG at the base 4bb8af77
(ledger:28, into .prd/07-fx-parity-program.md, a file this unit does not
touch) is left as it is: it is not this unit's.

    python3 -I -B repoint.py <repo> <code head>            # prints each one
    python3 -I -B repoint.py <repo> <code head> --apply    # writes them
"""
import re
import subprocess
import sys

LEDGER = "scripts/fxcap/parity-ledger.toml"
MOVED = {"src/tui/shell.rs", "src/tui/mcp_args.rs", "src/tui/mod.rs"}
LINE = re.compile(
    r"^WRONG scripts/fxcap/parity-ledger\.toml:(\d+) (\S*?):(\d+) \((path|bare); "
    r".* -- its text is at :(\d+)$"
)


def main():
    repo, head = sys.argv[1], sys.argv[2]
    apply = "--apply" in sys.argv
    out = subprocess.run(
        ["python3", "-I", "-B", ".prd/tui-phase5/receipts/train-13/refs_content.py", ".", head, LEDGER],
        cwd=repo, capture_output=True, text=True,
    ).stdout
    lines = open(f"{repo}/{LEDGER}", encoding="utf-8").read().split("\n")
    path_on = {}
    edits = []
    for row in out.splitlines():
        m = LINE.match(row)
        if not m:
            continue
        at, ref, old, kind, new = int(m[1]), m[2], m[3], m[4], m[5]
        # A bare `:N` is a ref into the file the path before it on the line names.
        if kind == "path":
            path_on[at] = ref
        target = ref if kind == "path" else path_on.get(at, "")
        if target not in MOVED:
            print(f"skip   ledger:{at} {ref}:{old} (not a file this unit moved)")
            continue
        token = f"{ref}:{old}" if kind == "path" else f":{old}"
        pattern = re.compile(r"(?<![\w./:])" + re.escape(token) + r"(?![\d-])") if kind == "bare" \
            else re.compile(re.escape(token) + r"(?![\d-])")
        hits = len(pattern.findall(lines[at - 1]))
        if hits != 1:
            sys.exit(f"ledger:{at}: {token} is there {hits} times")
        repl = f"{ref}:{new}" if kind == "path" else f":{new}"
        edits.append((at, pattern, repl))
        print(f"repoint ledger:{at} {token} -> {repl}")
    if apply:
        for at, pattern, repl in edits:
            lines[at - 1] = pattern.sub(repl, lines[at - 1], count=1)
        open(f"{repo}/{LEDGER}", "w", encoding="utf-8").write("\n".join(lines))
    print(f"{len(edits)} re-points{' written' if apply else ''}")


if __name__ == "__main__":
    main()
