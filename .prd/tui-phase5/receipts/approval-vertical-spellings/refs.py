"""Every `src/tui/<file>.rs:<line>` the parity ledger cites in a file this
unit changes, checked by **content**: each ref must name a line whose text is
the one its entry is about, and that text must sit on exactly one line.

Keyed by the ledger entry, not mapped from the base: at this unit's base
(2593f49c) twelve of the sixteen refs into src/tui/shell.rs named a line
that was not their entry's (a fault older than this unit; origin/main's ledger
names the right lines), so a base-to-head mapping would carry the fault
along. The base ref's own text is printed beside each, so the old fault
stays on record.

usage: python3 -I refs.py <base rev> [--fix]   (from the repo root)
Prints one line per ref and `refs: N checked, M wrong`; --fix writes the
right numbers into the ledger. Exits 1 when one is wrong without --fix, or
when a ref has no expected text below (a new ref must be added here).
"""

import re
import subprocess
import sys

sys.dont_write_bytecode = True
LEDGER = "scripts/fxcap/parity-ledger.toml"
FILES = "shell|input|editor|approval|approval_amendment"
REF = re.compile(r"(src/tui/(?:%s)\.rs):(\d+)(?![\d-])" % FILES)
# What each entry's ref is about: the line's text, stripped.
SEND = "fn send(&mut self, prompt: String, text: &str) -> bool {"
RESIZE = "pub(crate) fn resize(&mut self, rows: u16, cols: u16) -> Resize {"
TOOLS = '"  tools: {} advertised ({},",'
TRAILER = "fn trailer_rows(&self) -> Vec<String> {"
EXPECT = {
    "banner": '"{}x\\u{1d487}x{}{} {label} \\u{b7} Run /help for commands{}",',
    "answer-drawn-wider-named": "let event = full_transcript::shown_event(event);",
    "submit-multi-card-whole": SEND,
    "at-resize-band-at-the-bottom": RESIZE,
    "session-record": TOOLS,
    "transcript-page-offset": TOOLS,
    "toolgroup-ctrlo-record-short": TOOLS,
    "safe-page-edge": TOOLS,
    "cancel-page-edge": TOOLS,
    "amendment-card-after-stop": TRAILER,
    "full-transcript-shift": TOOLS,
    "stats-bytes": '"ansi_bytes={}, redraws={}, debounced_resizes={}, footer_updates=0, stream_chunks=0",',
    "resize-document-repaint": RESIZE,
    "activity-tokens-before-request": "self.tally.begin(Instant::now(), estimate);",
}


def show(rev, path):
    return subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True, text=True, check=True).stdout


def entry_ids(text):
    """(start, end, id) of each [[...]] entry."""
    starts = [m.start() for m in re.finditer(r"^\[\[", text, re.M)] + [len(text)]
    out = []
    for a, b in zip(starts, starts[1:]):
        m = re.search(r'^id = "([^"]+)"', text[a:b], re.M)
        out.append((a, b, m.group(1) if m else None))
    return out


def id_at(spans, pos):
    return next((i for a, b, i in spans if a <= pos < b), None)


def main():
    base = sys.argv[1]
    fix = "--fix" in sys.argv
    head_ledger = open(LEDGER, encoding="utf-8").read()
    base_ledger = show(base, LEDGER)
    head_spans, base_spans = entry_ids(head_ledger), entry_ids(base_ledger)
    files = sorted({m.group(1) for m in REF.finditer(head_ledger)})
    new = {f: open(f, encoding="utf-8").read().splitlines() for f in files}
    old = {f: show(base, f).splitlines() for f in files}
    # The base's refs, in order per entry, for the record.
    base_refs = {}
    for m in REF.finditer(base_ledger):
        base_refs.setdefault((id_at(base_spans, m.start()), m.group(1)), []).append(int(m.group(2)))
    seen = {}
    checked = wrong = unknown = 0
    edits = []
    for m in REF.finditer(head_ledger):
        path, got = m.group(1), int(m.group(2))
        eid = id_at(head_spans, m.start())
        line = head_ledger.count("\n", 0, m.start()) + 1
        checked += 1
        want = EXPECT.get(eid)
        if want is None:
            unknown += 1
            print(f"UNKNOWN ledger:{line} [{eid}] {path}:{got}: no expected text in refs.py")
            continue
        hits = [i + 1 for i, text in enumerate(new[path]) if text.strip() == want]
        held = new[path][got - 1].strip() if 0 < got <= len(new[path]) else ""
        k = seen.get((eid, path), 0)
        seen[(eid, path)] = k + 1
        was = base_refs.get((eid, path), [])
        was = was[k] if k < len(was) else None
        base_held = old[path][was - 1].strip() if was and was <= len(old[path]) else ""
        note = f"base {was} held {base_held[:40]!r}" if was else "no base ref"
        if held == want and hits == [got]:
            print(f"ok    ledger:{line} [{eid}] {path}:{got}  {want[:48]}  ({note})")
            continue
        wrong += 1
        print(f"WRONG ledger:{line} [{eid}] {path}:{got} holds {held[:40]!r}; {want[:40]!r} is at {hits}  ({note})")
        if len(hits) == 1:
            edits.append((m.start(), m.end(), f"{path}:{hits[0]}"))
    if fix and edits:
        out = head_ledger
        for start, end, text in sorted(edits, reverse=True):
            out = out[:start] + text + out[end:]
        open(LEDGER, "w", encoding="utf-8").write(out)
        print(f"fixed {len(edits)}")
    print(f"refs: {checked} checked, {wrong} wrong, {unknown} without an expected text")
    sys.exit(1 if unknown or (wrong and not fix) else 0)


if __name__ == "__main__":
    main()
