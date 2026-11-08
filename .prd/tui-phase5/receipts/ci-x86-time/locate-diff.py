"""usage: python3 -I locate-diff.py <macho> <cmp-l-offsets-file>
Every differing byte (1-based offsets from `cmp -l`) must lie inside the
Mach-O's LC_UUID or its code signature; read from the header itself."""
import struct
import sys

data = open(sys.argv[1], "rb").read()
diffs = [int(line) for line in open(sys.argv[2]) if line.strip()]
ncmds = struct.unpack_from("<I", data, 16)[0]
off, ranges = 32, {}
for _ in range(ncmds):
    cmd, size = struct.unpack_from("<II", data, off)
    if cmd == 0x1B:  # LC_UUID: 16 bytes after cmd and cmdsize
        ranges["LC_UUID"] = (off + 9, off + 24)
    if cmd == 0x1D:  # LC_CODE_SIGNATURE: dataoff, datasize
        dataoff, datasize = struct.unpack_from("<II", data, off + 8)
        ranges["code signature"] = (dataoff + 1, dataoff + datasize)
    off += size
spans, start, prev = [], diffs[0], diffs[0]
for d in diffs[1:]:
    if d != prev + 1:
        spans.append((start, prev))
        start = d
    prev = d
spans.append((start, prev))
print("      differing bytes: %d, in %s" % (len(diffs), " ".join("%d-%d" % s for s in spans)))
for name, (lo, hi) in ranges.items():
    print("      %s is bytes %d-%d" % (name, lo, hi))
outside = [d for d in diffs if not any(lo <= d <= hi for lo, hi in ranges.values())]
print("      differing bytes outside LC_UUID and the code signature: %d" % len(outside))
sys.exit(1 if outside else 0)
