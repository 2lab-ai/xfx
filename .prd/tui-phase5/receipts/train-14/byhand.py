#!/usr/bin/env python3
"""Train 14 (new; merge 1's `event_loop.rs` blocks are `eventloop.py`'s):
merge 3's conflict blocks that no rule of `blocks.py` fits, resolved by hand
and written down here, so `reproduce.sh` can redo them. The file is read in
git's diff3 style (ours, `|||||||` base, `=======` theirs). Each block in the
table is found by the sha256 (16 hex digits) of its ours, base and theirs
lines and replaced as the table says; a block not in the table is left for
`blocks.py`; a table entry not found exactly once stops the script, changing
nothing.

`src/tui/shell.rs`, five blocks, **ours in all five.** Both sides moved
`anchor_at` to the caret's row, each on its own: CTRLO-OTHER-PICKERS (ours
since merge 2) folded `column_anchor` into `anchor_at` and deleted it ("one
anchor function", its round 2, `3369622a`); PAGE-KEYS-SETTINGS-COLUMN
(theirs) routed `anchor_at` through `column_anchor` (`0312cf42`) and, in its
round 3 (`8e809d40`, comments only), corrected the fx lines both functions'
docs cite. Ours' `anchor_at` body is theirs' `column_anchor` body line for
line (the same `point_of`, the same caret-row test, the same `PROMPT_CELLS`
sum). Ours' docs cite the pin's lines (`visual_layout.zig:705-713@481104fd`,
`input_presentation.zig:383-386@481104fd`), the lines theirs corrected to.
Taking theirs' anchor would put back the function ours deleted; nothing
else calls it.

`scripts/fxcap/parity-ledger.toml`, one block: the base's `at-ranking` and
`at-folder-row` causes. Ours deleted both (AT-RANK closed `at-ranking`,
`a5b1cb59`; AT-FOLDERS deleted `at-folder-row`, `9294e3f2`) and wrote
`at-plan-run-directory` where they stood; theirs kept both and added
`at-ranking-anchor` between them (its round 4). Merged entry by entry: ours'
lines, a blank line, then theirs' `at-ranking-anchor` entry whole.

    python3 -I -B byhand.py <worktree-file>
"""
import hashlib
import sys

path = sys.argv[1]
OURS = "ours"


def anchor_entry(o, t):
    k = t.index('id = "at-ranking-anchor"')
    end = t.index("[[cause]]", k)
    return o + [""] + t[k - 1:end]


TABLE = {
    "src/tui/shell.rs": [
        (("9f7e0fbb4e5e4b9d", "0d0d1c03a7056d96", "c5035e1b287d0750"), OURS),
        (("f0ab7c7c959dbd33", "2f89912e581044b8", "bf4d50eae8fca179"), OURS),
        (("0155dbd9510264ab", "58903ff49afb31a3", "9d39c00376885ce8"), OURS),
        (("7b578bfc6d77f55a", "586ba082d7a1103c", "e5942b0e2e7dbfd0"), OURS),
        (("e3b0c44298fc1c14", "cc4769843c224d25", "52dd70f0ff4ac171"), OURS),
    ],
    "scripts/fxcap/parity-ledger.toml": [
        (("b7675396a905fb1f", "88b71d398081cb3f", "ce4c05f471f8956c"), anchor_entry),
    ],
}


def h(lines):
    return hashlib.sha256("\n".join(lines).encode()).hexdigest()[:16]


table = next((v for k, v in TABLE.items() if path.endswith(k)), None)
if table is None:
    sys.exit(f"{path}: no table")
found = {sides: 0 for sides, _ in table}
lines = open(path, encoding="utf-8").read().split("\n")
out, i, said = [], 0, []
while i < len(lines):
    if not lines[i].startswith("<<<<<<< "):
        out.append(lines[i])
        i += 1
        continue
    bar = next(j for j in range(i, len(lines)) if lines[j].startswith("||||||| "))
    mid = next(j for j in range(bar, len(lines)) if lines[j] == "=======")
    end = next(j for j in range(mid, len(lines)) if lines[j].startswith(">>>>>>> "))
    o, t = lines[i + 1:bar], lines[mid + 1:end]
    sides = (h(o), h(lines[bar + 1:mid]), h(t))
    rule = dict(table).get(sides)
    if rule is None:
        out.extend(lines[i:end + 1])
    else:
        found[sides] += 1
        merged = o if rule == OURS else rule(o, t)
        said.append(f"{path}:{len(out) + 1}: by hand, {'ours' if rule == OURS else rule.__name__} "
                    f"(ours {len(o)} lines, base {mid - bar - 1}, theirs {len(t)} -> {len(merged)})")
        out.extend(merged)
    i = end + 1
if any(n != 1 for n in found.values()):
    sys.exit(f"{path}: table entries found {list(found.values())}, not once each -- read by hand")
open(path, "w", encoding="utf-8").write("\n".join(out))
print("\n".join(said))
print(f"# {path}: {len(said)} blocks resolved by hand")
