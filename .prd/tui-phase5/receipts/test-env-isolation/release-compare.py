#!/usr/bin/env python3
"""Where two release builds of xfx differ, byte by byte (macOS Mach-O).

    release-compare.py <base binary> <head binary>

Both built with XFX_BUILD_REVISION pinned to the same value, so the banner
revision is not a difference. Prints the sizes, then every differing byte by
where it is: a section, the Mach-O header (LC_UUID), or the code signature.
For __DATA_CONST,__const it decodes each change as a panic Location
(file, line, column) -- a line number that moved because a #[cfg(test)]
line was added above it. Reads the two files only.
"""
import re
import struct
import subprocess
import sys
from collections import Counter


def sections(path):
    out = subprocess.run(["otool", "-l", path], capture_output=True, text=True).stdout
    found, cur, uuid_cmd, sig = [], {}, False, None
    lines = [line.strip() for line in out.splitlines()]
    for i, line in enumerate(lines):
        m = re.match(r"(sectname|segname|offset|size|addr|cmd|dataoff|datasize) (\S+)", line)
        if not m:
            continue
        key, value = m.groups()
        if key == "cmd" and value == "LC_CODE_SIGNATURE":
            off = int(re.search(r"dataoff (\d+)", " ".join(lines[i:i + 4])).group(1))
            size = int(re.search(r"datasize (\d+)", " ".join(lines[i:i + 4])).group(1))
            sig = (off, off + size)
        if key == "sectname":
            cur = {"sect": value}
        elif key == "segname" and "sect" in cur:
            cur["seg"] = value
        elif key == "addr" and "sect" in cur:
            cur["addr"] = int(value, 16)
        elif key == "size" and "sect" in cur:
            cur["size"] = int(value, 16)
        elif key == "offset" and "sect" in cur:
            cur["off"] = int(value)
            found.append(cur)
            cur = {}
    return found, sig


def uuid_range(data):
    ncmds = struct.unpack_from("<I", data, 16)[0]
    at = 32
    for _ in range(ncmds):
        cmd, size = struct.unpack_from("<II", data, at)
        if cmd == 0x1B:  # LC_UUID
            return (at + 8, at + 24)
        at += size
    return None


def main():
    base_path, head_path = sys.argv[1], sys.argv[2]
    a = open(base_path, "rb").read()
    b = open(head_path, "rb").read()
    print(f"size base {len(a)} head {len(b)}")
    assert len(a) == len(b), "the sizes differ"
    secs, sig = sections(base_path)
    secs_b, sig_b = sections(head_path)
    print("section table identical (name, address, size, offset):",
          [(s["seg"], s["sect"], s["addr"], s["size"], s["off"]) for s in secs]
          == [(s["seg"], s["sect"], s["addr"], s["size"], s["off"]) for s in secs_b])
    uuid = uuid_range(a)
    diffs = [i for i in range(len(a)) if a[i] != b[i]]
    print(f"differing bytes {len(diffs)}")

    def where(offset):
        for s in secs:
            if s["off"] <= offset < s["off"] + s["size"]:
                return f"{s['seg']},{s['sect']}"
        if uuid and uuid[0] <= offset < uuid[1]:
            return "header LC_UUID"
        if sig and sig[0] <= offset < sig[1]:
            return "code signature (page hashes)"
        return "elsewhere"

    for name, count in Counter(where(o) for o in diffs).most_common():
        print(f"  {count:5d} {name}")
    const = next(s for s in secs if s["seg"] == "__DATA_CONST" and s["sect"] == "__const")
    seen, moved, unexplained = set(), Counter(), 0
    for offset in diffs:
        if not const["off"] <= offset < const["off"] + const["size"]:
            continue
        explained = False
        for base in range(offset - offset % 8 - 16, offset - offset % 8 + 1, 8):
            pa, la, line_a, col_a = struct.unpack_from("<QQII", a, base)
            pb, lb, line_b, col_b = struct.unpack_from("<QQII", b, base)
            if pa == pb and la == lb and 0 < la < 200 and col_a == col_b and line_a != line_b:
                home = next((s for s in secs if s["addr"] <= pa < s["addr"] + s["size"]), None)
                name = a[home["off"] + pa - home["addr"]:][:la].decode("utf8", "replace") if home else "?"
                if base not in seen:
                    seen.add(base)
                    moved[(name, line_b - line_a)] += 1
                explained = True
                break
        unexplained += not explained
    print("__DATA_CONST,__const: panic Locations whose line moved (file, +lines): count")
    for (name, delta), count in sorted(moved.items()):
        print(f"  {name} +{delta}: {count}")
    print(f"__DATA_CONST,__const bytes not explained as a moved line: {unexplained}")


main()
