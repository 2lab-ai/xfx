"""fx's probe against xfx's, fixture by fixture, two ways.

- **Refusal**: at `ask` and at the interactive launch (xfx's TUI and its line
  shell, each against fx's interactive launch), whether the launch is
  refused over the profile: `InvalidProfileConfiguration`, or the parse
  error fx names for a bad `provider` or `models` (`Invalid...`,
  `Unknown...`), right after the program's name.
- **Exit**: at the same entries, how the run ended: `ask`'s exit status, and
  for an interactive launch its exit status or that it was still running.

Prints each fixture that differs and why, then both counts.

usage: python3 -I outcomes.py <fx probe> <xfx probe>
"""

import re
import sys

sys.dont_write_bytecode = True


def load(path):
    runs, name = {}, None
    for line in open(path, encoding="utf-8"):
        head = re.match(r"^== (\S+)$", line.rstrip("\n"))
        if head:
            name = head.group(1)
            runs[name] = {}
            continue
        entry = re.match(r"^  (ask|interactive|line shell|permissions)\s+(.*)$", line.rstrip("\n"))
        if name and entry:
            rest = entry.group(2)
            state = "running" if "still running" in rest else "exit=" + (re.search(r"exit=(\d*)", rest) or [None, "?"])[1]
            refused = re.search(r"(?:stderr=|\|)x?fx: (?:Invalid|Unknown)[A-Za-z]+\|", rest) is not None
            runs[name][entry.group(1)] = (refused, state)
    return runs


def main():
    fx, xfx = load(sys.argv[1]), load(sys.argv[2])
    names = [n for n in fx if n in xfx]
    by_refusal = by_exit = 0
    for name in names:
        f, x = fx[name], xfx[name]
        pairs = [("ask", "ask"), ("interactive", "interactive"), ("interactive", "line shell")]
        refusal = [(fe, xe) for fe, xe in pairs if xe in x and f[fe][0] != x[xe][0]]
        exit_ = [(fe, xe) for fe, xe in pairs if xe in x and f[fe][1] != x[xe][1]]
        by_refusal += not refusal
        by_exit += not exit_
        if refusal or exit_:
            why = []
            for fe, xe in refusal:
                why.append(f"refusal at {xe}: fx {f[fe][0]} xfx {x[xe][0]}")
            for fe, xe in exit_:
                why.append(f"end at {xe}: fx {f[fe][1]} xfx {x[xe][1]}")
            print(f"{name}: " + "; ".join(why))
    print(f"== {len(names)} fixtures: {by_refusal} match on refusal at every entry, {by_exit} on how every entry ended")


if __name__ == "__main__":
    main()
