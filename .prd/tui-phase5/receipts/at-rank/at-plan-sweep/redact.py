"""The head sweep's `workspace-index` `at-plan` captures, with the sweep's
scratch root and unit directory (`<scratch>/atrs/fxsw.<id>/039`) written
`<tmp>` wherever it stands, an SGR sequence inside it kept in order after
`<tmp>`; the run directory's own name (`fxcap.qnVERa`, `fxcap.YTZTDF`) is
kept, since it is what the bold letters depend on. Then each side's row 13
is printed with its bold letters in brackets (bold.txt).

usage: python3 -I redact.py <src dir> <dst dir> <root>   (redact)
       python3 -I redact.py --bold <dst dir>             (bold.txt on stdout)
"""
import os, re, sys
sys.dont_write_bytecode = True
SGR = r"(?:\x1b\[[0-9;]*m)*"
NAMES = ["fx-workspace-index-40x120-at-plan", "xfx-workspace-index-40x120-at-plan"]

def redact(src, dst, root):
    pat = "".join(re.escape(ch) + SGR for ch in root)
    for name in NAMES:
        for ext in ("ansi", "txt"):
            text = open(os.path.join(src, name + "." + ext), encoding="utf-8").read()
            text = re.sub(pat, lambda m: "<tmp>" + "".join(re.findall(r"\x1b\[[0-9;]*m", m.group(0))), text)
            open(os.path.join(dst, name + "." + ext), "w", encoding="utf-8").write(text)

def bold(dst):
    for name in NAMES:
        rows = open(os.path.join(dst, name + ".ansi"), encoding="utf-8").read().split("\n")
        row = rows[12]
        row = re.sub(r"\x1b\[1m(.*?)\x1b\[0m", lambda m: "[" + re.sub(r"\x1b\[[0-9;]*m", "", m.group(1)) + "]", row)
        print("%-5s row 13: %s" % (name.split("-")[0], re.sub(r"\x1b\[[0-9;]*m", "", row).rstrip()))

if sys.argv[1] == "--bold":
    bold(sys.argv[2])
else:
    redact(sys.argv[1], sys.argv[2], sys.argv[3])
