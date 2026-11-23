"""fx's probe against xfx's (`probe.sh`), fixture by fixture.
(PROFILE-REFUSE-TYPED-KEYS's `outcomes.py`, with this probe's headers and two
more comparisons.)

- **Refusal**: at `ask`, `status` and the interactive launch (xfx's TUI and
  its line shell, each against fx's interactive launch), the error's name the
  run stopped with over the profile (`InvalidProfileConfiguration`), or none.
- **Exit**: at the same entries, the exit status, or that an interactive
  launch was still running.
- **Step limit**: `status`'s `agent_step_limit`, as `default` when it is the
  product's own default (fx 0, xfx 25) and as the number otherwise: whether
  the file's `max_agent_steps` was applied.
- **Notes**: the `config <layer>: <cause>` lines `ask` wrote on stderr.

Prints each fixture that differs and how, then the counts.

usage: python3 -I outcomes.py <fx probe> <xfx probe>
"""

import re
import sys

sys.dont_write_bytecode = True

DEFAULT_STEPS = {"fx": "0", "xfx": "25"}


def load(path, product):
    runs, name = {}, None
    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        head = re.match(r"^== (\S+) \(", line)
        if head:
            name = head.group(1)
            runs[name] = {"steps": None, "notes": ()}
            continue
        if not name:
            continue
        steps = re.match(r"^    \| \[status\] agent_step_limit=(\d+)$", line)
        if steps:
            value = steps.group(1)
            runs[name]["steps"] = "default" if value == DEFAULT_STEPS[product] else value
            continue
        entry = re.match(r"^  (ask|status|interactive|line shell|permissions)\s+(.*)$", line)
        if entry:
            rest = entry.group(2)
            state = "running" if "still running" in rest else "exit=" + (re.search(r"exit=(\d*)", rest) or [None, "?"])[1]
            named = re.search(r"(?:stderr=|\|)x?fx: ([A-Z][A-Za-z]+)\|", rest)
            runs[name][entry.group(1)] = (named.group(1) if named else None, state)
            if entry.group(1) == "ask":
                runs[name]["notes"] = tuple(re.findall(r"config (\w+): (\w+)", rest))
    return runs


def main():
    fx, xfx = load(sys.argv[1], "fx"), load(sys.argv[2], "xfx")
    names = [n for n in fx if n in xfx]
    same = {"refusal": 0, "exit": 0, "steps": 0, "notes": 0}
    pairs = [("ask", "ask"), ("status", "status"), ("interactive", "interactive"), ("interactive", "line shell")]
    for name in names:
        f, x = fx[name], xfx[name]
        missing = [e for fe, xe in pairs for e in (("fx", fe, f), ("xfx", xe, x)) if e[1] not in e[2]]
        if missing:
            print(f"{name}: not run at " + ", ".join(f"{who} {entry}" for who, entry, _ in missing))
            continue
        why = []
        refusal = [(fe, xe) for fe, xe in pairs if f[fe][0] != x[xe][0]]
        exit_ = [(fe, xe) for fe, xe in pairs if f[fe][1] != x[xe][1]]
        for fe, xe in refusal:
            why.append(f"refusal at {xe}: fx {f[fe][0]} xfx {x[xe][0]}")
        for fe, xe in exit_:
            why.append(f"end at {xe}: fx {f[fe][1]} xfx {x[xe][1]}")
        if f["steps"] != x["steps"]:
            why.append(f"step limit: fx {f['steps']} xfx {x['steps']}")
        if f["notes"] != x["notes"]:
            why.append(f"notes: fx {list(f['notes'])} xfx {list(x['notes'])}")
        same["refusal"] += not refusal
        same["exit"] += not exit_
        same["steps"] += f["steps"] == x["steps"]
        same["notes"] += f["notes"] == x["notes"]
        if why:
            print(f"{name}: " + "; ".join(why))
    print(
        f"== {len(names)} fixtures: {same['refusal']} match on refusal at every entry, "
        f"{same['exit']} on how every entry ended, {same['steps']} on the step limit, "
        f"{same['notes']} on the notes"
    )


if __name__ == "__main__":
    main()
