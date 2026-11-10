"""fx's probe against xfx's, fixture by fixture: the mode each entry reports.
(PROFILE-REFUSE-TYPED-KEYS's outcomes.py, comparing the applied mode where
that one compared the refusal.)

- **status**, **status --json**, **doctor** (its `[doctor] permission_mode=`
  and its startup check): the mode word, as printed. No normalisation: the
  text lines say `full access`, the JSON says `yolo`, on both products.
- **ask**: how the run went on, from its first stderr line --
  `refused <Name>` (an error's name right after the program's name),
  `full access` (fx's `Full access enabled: ...`, xfx's `xfx: yolo mode is
  on ...`), `parser` (fx's `usage: fx ask ...`, clap's `error: ...`), or
  `ran` (it went on to the missing credential).
- fx's `permissions` entry has no xfx counterpart; where it prints a mode it
  is checked against fx's own `status`, a third witness of the mode fx
  applied (it prints none for a profile fx refuses).

Prints each fixture that differs and why, then the counts.

usage: python3 -I outcomes.py <fx probe> <xfx probe>
"""

import re
import sys

sys.dont_write_bytecode = True


def ask_outcome(stderr):
    named = re.match(r"^x?fx: ([A-Z][A-Za-z]+)$", stderr)
    if named:
        return "refused " + named.group(1)
    if stderr.startswith("Full access enabled:") or stderr.startswith("xfx: yolo mode is on"):
        return "full access"
    if stderr.startswith("usage: fx ask") or stderr.startswith("error: "):
        return "parser"
    return "ran"


def load(path):
    runs, name = {}, None
    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        head = re.match(r"^== (\S+)$", line)
        if head:
            name = head.group(1)
            runs[name] = {}
            continue
        entry = re.match(r"^  (permissions|status --json|status|doctor|ask(?: [^|]*?)?)\s+exit=(\d*) (.*?)\| stderr=(.*)$", line)
        if not (name and entry):
            continue
        label, modes, stderr = entry.group(1).strip(), entry.group(3), entry.group(4)
        facts = dict(kv.split("=", 1) for kv in modes.split(";") if "=" in kv)
        if label.startswith("ask"):
            runs[name]["ask"] = ask_outcome(stderr)
        elif label == "status --json":
            runs[name]["json"] = facts.get("json")
        elif label == "doctor":
            runs[name]["doctor"] = facts.get("doctor")
            runs[name]["startup"] = facts.get("startup")
        else:
            runs[name][label] = facts.get(label)
    return runs


def main():
    fx, xfx = load(sys.argv[1]), load(sys.argv[2])
    names = [n for n in fx if n in xfx]
    same = 0
    for name in names:
        f, x = fx[name], xfx[name]
        why = [f"{k}: fx {f[k]!r} xfx {x.get(k)!r}" for k in f if k != "permissions" and f[k] != x.get(k)]
        # None: fx's `permissions` refused the profile (`ask` says by what name).
        if f.get("permissions") is not None and f["permissions"] != f.get("status"):
            why.append(f"fx's own permissions {f['permissions']!r} against its status {f.get('status')!r}")
        same += not why
        if why:
            print(f"{name}: " + "; ".join(why))
    print(f"== {len(names)} fixtures: {same} report the same mode at every entry, {len(names) - same} do not")


if __name__ == "__main__":
    main()
