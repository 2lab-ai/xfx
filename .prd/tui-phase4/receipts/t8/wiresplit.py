#!/usr/bin/env python3
"""Split a captured .wire into synchronized-update blocks and the bytes outside them.

usage: wiresplit.py <file.wire> [--from N] [--all] [--text]

Each segment is printed as `sync|outside <bytes>  <tokens>`; tokens are CSI/OSC
named, SGR dropped, runs of linefeeds and of CUP+erase pairs compressed. With
--text, text runs are shown (clipped); otherwise as <tN>.
"""
import re
import sys

ESC = 0x1B


def tokens(data, show_text):
    out = []
    i = 0
    n = len(data)
    while i < n:
        b = data[i]
        if b == ESC and i + 1 < n and data[i + 1] == ord("["):
            j = i + 2
            while j < n and not (0x40 <= data[j] <= 0x7E):
                j += 1
            seq = data[i + 2 : j + 1].decode("latin1")
            i = j + 1
            final = seq[-1]
            params = seq[:-1]
            if final == "m":
                continue
            if final in "hl" and params.startswith("?"):
                out.append(params + final)
            elif final == "H":
                out.append("CUP(%s)" % (params.replace(";", ",") or "1,1"))
            elif final == "K":
                out.append({"": "EL", "0": "EL", "2": "2K", "1": "1K"}.get(params, "K" + params))
            elif final == "J":
                out.append({"": "ED", "0": "0J", "2": "2J", "3": "3J"}.get(params, "J" + params))
            else:
                out.append("CSI " + seq)
        elif b == ESC and i + 1 < n and data[i + 1] == ord("]"):
            j = i + 2
            while j < n and data[j] != 0x07 and not (data[j] == ESC and j + 1 < n and data[j + 1] == ord("\\")):
                j += 1
            body = data[i + 2 : j].decode("utf8", "replace")
            i = j + (1 if j < n and data[j] == 0x07 else 2)
            if body.startswith("2;"):
                out.append('OSC2"%s"' % body[2:])
            elif body.startswith("8;"):
                out.append("OSC8" + ("" if body.endswith(";") else "+"))
            else:
                out.append("OSC " + body[:20])
        elif b == ESC:
            out.append("ESC " + chr(data[i + 1]) if i + 1 < n else "ESC")
            i += 2
        elif b == 0x0A:
            out.append("LF")
            i += 1
        elif b == 0x0D:
            out.append("CR")
            i += 1
        elif b == 0x07:
            out.append("BEL")
            i += 1
        elif b < 0x20:
            out.append("C0(%02x)" % b)
            i += 1
        else:
            j = i
            while j < n and data[j] >= 0x20 and data[j] != 0x7F and data[j] != ESC:
                j += 1
            text = data[i:j].decode("utf8", "replace")
            out.append(('"%s"' % (text if len(text) <= 24 else text[:21] + "..")) if show_text else "<t%d>" % len(text))
            i = j
    return out


def compress(toks):
    out = []
    i = 0
    while i < len(toks):
        t = toks[i]
        if t == "LF":
            j = i
            while j < len(toks) and toks[j] == "LF":
                j += 1
            out.append("[LF x%d]" % (j - i) if j - i > 1 else "LF")
            i = j
            continue
        if t.startswith("CUP(") and i + 1 < len(toks) and toks[i + 1] in ("2K", "EL"):
            kind = toks[i + 1]
            j = i
            while j + 1 < len(toks) and toks[j].startswith("CUP(") and toks[j + 1] == kind:
                j += 2
            count = (j - i) // 2
            if count > 1:
                out.append("[CUP+%s x%d]" % (kind, count))
                i = j
                continue
        out.append(t)
        i += 1
    return out


def segments(data):
    """(kind, start, end) over the whole capture."""
    segs = []
    i = 0
    open_tok = b"\x1b[?2026h"
    close_tok = b"\x1b[?2026l"
    while i < len(data):
        o = data.find(open_tok, i)
        if o < 0:
            segs.append(("outside", i, len(data)))
            break
        if o > i:
            segs.append(("outside", i, o))
        c = data.find(close_tok, o)
        if c < 0:
            segs.append(("UNCLOSED", o, len(data)))
            break
        segs.append(("sync", o, c + len(close_tok)))
        i = c + len(close_tok)
    return segs


def writes_cells(toks):
    return any(t == "LF" or t.startswith("[LF") or t.startswith("<t") or t.startswith('"') for t in toks)


def main():
    path = sys.argv[1]
    start = 0
    show_all = "--all" in sys.argv
    show_text = "--text" in sys.argv
    if "--from" in sys.argv:
        start = int(sys.argv[sys.argv.index("--from") + 1])
    data = open(path, "rb").read()[start:]
    segs = segments(data)
    syncs = [s for s in segs if s[0] == "sync"]
    outs = [s for s in segs if s[0] != "sync"]
    painting_outside = []
    for kind, a, b in outs:
        toks = tokens(data[a:b], False)
        if writes_cells(toks):
            painting_outside.append((a, b))
    print("# %s: %d bytes, %d sync blocks, %d outside segments, %d outside segments that write cells or scroll"
          % (path.split("/")[-1], len(data), len(syncs), len(outs), len(painting_outside)))
    for kind, a, b in segs:
        toks = compress(tokens(data[a:b], show_text))
        if not show_all and kind == "outside" and not toks:
            continue
        line = "%-8s %6d bytes @%-6d %s" % (kind, b - a, a + start, " ".join(toks))
        print(line if show_all or len(line) < 400 else line[:400] + " ...")


if __name__ == "__main__":
    main()
