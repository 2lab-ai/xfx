"""Every `src/tui/<file>.rs:<line>` the parity ledger cites in a file this
unit changes, checked at the checkout. The ledger keeps its order, so the
n-th such ref outside this unit's own causes stands for the base ledger's
n-th; it must name a line holding the text that base ref's line held, and
that line is found by mapping the base file onto the checkout's (difflib).
A ref in one of this unit's causes is checked against the text it is for.

usage: python3 -I refs.py <base rev> [--fix]   (CTRLO-OTHER-PICKERS: base cb7c9c18)

CTRLO-AT-KEYS' (AT-LAUNCH-WALK's), unchanged but for this paragraph, the
usage line and the file list: CTRLO-OTHER-PICKERS changes `shell.rs` and
`full_transcript.rs`, so `full_transcript.rs` is among the files.
Prints one line per ref and `refs: N checked, M wrong`; --fix writes the
mapped numbers into the ledger. Exits 1 when one is wrong without --fix.
"""

import difflib
import re
import subprocess
import sys

sys.dont_write_bytecode = True
LEDGER = "scripts/fxcap/parity-ledger.toml"
REF = re.compile(r"(src/tui/(?:shell|full_transcript|file_picker|file_index|event_loop|mod)\.rs):(\d+)(?:-(\d+))?(?![\d-])")
OWN = set()  # CTRLO-OTHER-PICKERS adds no cause with a ref; every ref maps from the base
# What a ref in this unit's causes is for.
OWN_TEXT = {"src/tui/shell.rs": "pub(crate) fn resize(&mut self, rows: u16, cols: u16) -> Resize {"}


def show(rev, path):
    return subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True, text=True, check=True).stdout


def entries(text):
    """(cause id or None, start, end) spans of the ledger's [[...]] entries."""
    starts = [m.start() for m in re.finditer(r"^\[\[", text, re.M)] + [len(text)]
    spans = []
    for a, b in zip(starts, starts[1:]):
        m = re.search(r'^id = "([^"]+)"', text[a:b], re.M)
        spans.append((m.group(1) if m else None, a, b))
    return spans


def own(text, pos):
    return any(a <= pos < b and cid in OWN for cid, a, b in entries(text))


def main():
    base = sys.argv[1]
    fix = "--fix" in sys.argv
    base_ledger = show(base, LEDGER)
    head_ledger = open(LEDGER, encoding="utf-8").read()
    files = sorted({m.group(1) for m in REF.finditer(base_ledger + head_ledger)})
    old = {f: show(base, f).splitlines() for f in files}
    new = {f: open(f, encoding="utf-8").read().splitlines() for f in files}
    mapping = {}
    for f in files:
        sm = difflib.SequenceMatcher(None, old[f], new[f], autojunk=False)
        mapping[f] = {a + k + 1: b + k + 1 for a, b, n in sm.get_matching_blocks() for k in range(n)}
    base_refs = [m for m in REF.finditer(base_ledger)]
    head_refs = [m for m in REF.finditer(head_ledger) if not own(head_ledger, m.start())]
    own_refs = [m for m in REF.finditer(head_ledger) if own(head_ledger, m.start())]
    if len(base_refs) != len(head_refs):
        sys.exit(f"the ledger's refs changed in number: {len(base_refs)} at the base, {len(head_refs)} here")
    checked = wrong = 0
    edits = []
    for b, h in zip(base_refs, head_refs):
        path = b.group(1)
        assert h.group(1) == path, (b.group(0), h.group(0))
        ends_b = [int(b.group(2))] + ([int(b.group(3))] if b.group(3) else [])
        ends_h = [int(h.group(2))] + ([int(h.group(3))] if h.group(3) else [])
        assert len(ends_b) == len(ends_h), (b.group(0), h.group(0))
        checked += 1
        texts = [old[path][n - 1].strip() for n in ends_b]
        wants = [mapping[path].get(n) for n in ends_b]
        helds = [new[path][g - 1].strip() if g <= len(new[path]) else "" for g in ends_h]
        if helds == texts and ends_h == wants:
            print(f"ok    {h.group(0)} (base {b.group(0).split(':', 1)[1]})  {texts[0][:60]}")
            continue
        wrong += 1
        print(f"WRONG {h.group(0)} (base {b.group(0).split(':', 1)[1]}) holds {helds[0][:40]!r}; {texts[0][:40]!r} is at {wants}")
        if all(wants):
            edits.append((h.start(), h.end(), f"{path}:" + "-".join(str(w) for w in wants)))
    for h in own_refs:
        path, got = h.group(1), int(h.group(2))
        text = OWN_TEXT[path]
        held = new[path][got - 1].strip() if got <= len(new[path]) else ""
        checked += 1
        hits = [i + 1 for i, line in enumerate(new[path]) if line.strip() == text]
        if held == text:
            print(f"ok    {path}:{got} (this unit's)  {text[:60]}")
            continue
        wrong += 1
        print(f"WRONG {path}:{got} (this unit's) holds {held[:40]!r}; {text[:40]!r} is at {hits}")
        if len(hits) == 1:
            edits.append((h.start(), h.end(), f"{path}:{hits[0]}"))
    if fix and edits:
        out = head_ledger
        for start, end, text in sorted(edits, reverse=True):
            out = out[:start] + text + out[end:]
        open(LEDGER, "w", encoding="utf-8").write(out)
        print(f"fixed {len(edits)}")
    print(f"refs: {checked} checked, {wrong} wrong")
    sys.exit(1 if wrong and not fix else 0)


if __name__ == "__main__":
    main()
