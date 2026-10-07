"""Row-by-row .ansi comparison of the P1 captures, fx vs xfx.

For each snap: every screen row compared byte for byte after the banner row,
rows aligned from the top; then again with the provider words mapped
(fx -> xfx): `Codex subscription` -> `llmux`, `gpt-5.6-sol` -> `fake/model-a`
in a notice, `gpt-5.6-sol` -> `model-a` on the hint row, `272k` -> `200k`.
"""
import sys

R = sys.argv[1]
SNAPS = {
    "provider-carry": ["start", "turn1", "open", "switched", "turn2", "quit", "resumed", "turn3"],
    "provider-carry-back": ["start", "turn1", "open", "switched", "turn2", "turn3"],
}
MAP = [
    ("Codex subscription with gpt-5.6-sol", "llmux with fake/model-a"),
    ("Codex subscription", "llmux"),
    ("gpt-5.6-sol", "model-a"),
    ("1k/272k", "1k/200k"),
]


def rows(path):
    lines = open(path, encoding="utf-8").read().split("\n")
    while lines and not lines[-1].strip():
        lines.pop()
    return lines


def mapped(line):
    for a, b in MAP:
        line = line.replace(a, b)
    return line


for scenario, snaps in SNAPS.items():
    for snap in snaps:
        fx = rows(f"{R}/fx-{scenario}-40x120-{snap}.ansi")
        xf = rows(f"{R}/xfx-{scenario}-40x120-{snap}.ansi")
        # The banner is each product's; rows after it are compared.
        start = 1 if snap not in ("resumed", "turn3") or scenario != "provider-carry" else 0
        a, b = fx[start:], xf[start:]
        if "--drop" in sys.argv:
            # The two rows that are not this unit's to match: fx's gated
            # sign-in leaf (`* auth: Switched credential ...`) and xfx's
            # one-time reasoning notice (`! session: ...`), each with the
            # blank row after it.
            def drop(lines, needle, count):
                out, skip = [], 0
                for line in lines:
                    if skip:
                        skip -= 1
                        continue
                    if needle in line:
                        skip = count - 1
                        continue
                    out.append(line)
                return out
            a = drop(a, "Switched credential", 2)
            b = drop(b, "session:", 3)
            # A row's colour close, where tmux puts it, is not the row.
            a = [line.replace("\x1b[39m", "") for line in a]
            b = [line.replace("\x1b[39m", "") for line in b]
        n = max(len(a), len(b))
        raw = [i for i in range(n) if (a[i] if i < len(a) else None) != (b[i] if i < len(b) else None)]
        mapd = [i for i in range(n) if (mapped(a[i]) if i < len(a) else None) != (b[i] if i < len(b) else None)]
        print(
            f"{scenario} {snap}: rows {len(a)} vs {len(b)}; "
            f"identical {n - len(raw)}/{n}; after the provider map {n - len(mapd)}/{n}; "
            f"differ at {[d + 1 + start for d in mapd]}"
        )
