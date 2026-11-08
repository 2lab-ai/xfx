"""usage: python3 -I objname-diff.py <macho-a> <macho-b>
Two same-size 64-bit Mach-O builds of one unit. Exits 0 only if every byte
that differs sits in the code signature, or in a symbol-table string naming an
object file (`<unit>.<cgu>.<token>.rcgu.o`, the debug map of an unstripped
build) whose two spellings differ only in <token>. Prints what it found."""
import re
import struct
import sys

a = open(sys.argv[1], "rb").read()
b = open(sys.argv[2], "rb").read()
if len(a) != len(b):
    print("      sizes differ: %d / %d" % (len(a), len(b)))
    sys.exit(1)
ncmds = struct.unpack_from("<I", a, 16)[0]
off, strtab, sig = 32, None, None
for _ in range(ncmds):
    cmd, size = struct.unpack_from("<II", a, off)
    if cmd == 0x2:  # LC_SYMTAB
        _, _, stroff, strsize = struct.unpack_from("<IIII", a, off + 8)
        strtab = (stroff, stroff + strsize)
    if cmd == 0x1D:  # LC_CODE_SIGNATURE
        do, ds = struct.unpack_from("<II", a, off + 8)
        sig = (do, do + ds)
    off += size
OBJ = re.compile(rb"^(.*\.)([0-9a-z]+)(\.rcgu\.o)$")
in_sig = in_names = other = 0
names = set()
bad = []
for i in range(len(a)):
    if a[i] == b[i]:
        continue
    if sig and sig[0] <= i < sig[1]:
        in_sig += 1
        continue
    if strtab and strtab[0] <= i < strtab[1]:
        lo = a.rfind(b"\0", strtab[0], i) + 1
        hi = a.find(b"\0", i)
        sa, sb = a[lo:hi], b[lo:hi]
        ma, mb = OBJ.match(sa), OBJ.match(sb)
        if ma and mb and ma.group(1) == mb.group(1) and ma.group(3) == mb.group(3):
            in_names += 1
            names.add(lo)
            continue
    other += 1
    if len(bad) < 3:
        bad.append(i + 1)
print("      differing bytes in the code signature: %d" % in_sig)
print("      differing bytes in %d object-file names, only in their per-build token: %d" % (len(names), in_names))
print("      differing bytes anywhere else (code, data, other strings): %d%s" % (other, (" e.g. at %s" % bad) if bad else ""))
sys.exit(1 if other else 0)
