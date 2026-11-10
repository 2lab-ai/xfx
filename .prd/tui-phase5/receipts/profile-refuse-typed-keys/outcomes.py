"""fx's probe against xfx's, fixture by fixture, two ways.
(PROFILE-REFUSE-AT-LAUNCH's, with `status` compared and the error's name.)

- **Refusal**: at `ask`, `status` and the interactive launch (xfx's TUI and
  its line shell, each against fx's interactive launch), the error the run
  stopped with over the profile, or none: an error's name right after the
  program's name (`InvalidProfileConfiguration`, `UnknownConfiguredProvider`,
  `InvalidModelValue`, `RetiredSkillMatchFuzzy`, ...; not a later failure
  such as `NotDir`). The names must be equal.
- **Exit**: at the same entries, how the run ended: the exit status, and for
  an interactive launch its exit status or that it was still running.

Prints each fixture that differs and why, then both counts.

usage: python3 -I outcomes.py <fx probe>[,<fx probe>...] <xfx probe>
  A later fx probe's fixture replaces an earlier one's.
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
        entry = re.match(r"^  (ask|status|interactive|line shell|permissions)\s+(.*)$", line.rstrip("\n"))
        if name and entry:
            rest = entry.group(2)
            state = "running" if "still running" in rest else "exit=" + (re.search(r"exit=(\d*)", rest) or [None, "?"])[1]
            named = re.search(r"(?:stderr=|\|)x?fx: ((?:Invalid|Unknown|Retired)[A-Za-z]+)\|", rest)
            refused = named.group(1) if named else None
            runs[name][entry.group(1)] = (refused, state)
    return runs


def main():
    fx = {}
    for path in sys.argv[1].split(","):
        fx.update(load(path))
    xfx = load(sys.argv[2])
    names = [n for n in fx if n in xfx]
    by_refusal = by_exit = 0
    for name in names:
        f, x = fx[name], xfx[name]
        pairs = [("ask", "ask"), ("status", "status"), ("interactive", "interactive"), ("interactive", "line shell")]
        pairs = [(fe, xe) for fe, xe in pairs if fe in f]
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
