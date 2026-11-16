"""SLASH-MIXED-SKILLS: fx's command table at the pin, as fx-src.txt quotes it
(section M11, `src/builtins/commands.zig:441-481@481104fd`), against xfx's
`SLASH_REGISTRY` (`src/interactive.rs`) at a revision: each command's name,
its aliases and whether it takes an argument. Which names take one decides
which lines list skills past their blank (`slashCompletionPrefix`).

usage: python3 -I has_args.py <repo> <rev>   (run from anywhere; reads fx-src.txt beside it)
"""
import os
import re
import subprocess
import sys

here = os.path.dirname(os.path.abspath(__file__))
repo, rev = sys.argv[1], sys.argv[2]

text = open(os.path.join(here, "fx-src.txt"), encoding="utf-8").read()
section = text[text.index("## M11 "):]
section = section[:section.index("### ", section.index("src/builtins/commands.zig") + 1)]
fx = {}
for line in section.splitlines():
    m = re.search(r'\.command = "([^"]+)"', line)
    if not m:
        continue
    aliases = re.search(r'\.aliases = &\.\{([^}]*)\}', line)
    names = re.findall(r'"([^"]+)"', aliases.group(1)) if aliases else []
    fx[m.group(1)] = (".has_args = true" in line, tuple(names))

src = subprocess.run(["git", "-C", repo, "show", f"{rev}:src/interactive.rs"],
                     capture_output=True, text=True, check=True).stdout
table = src[src.index("pub const SLASH_REGISTRY"):]
table = table[:table.index("\n];")]
xfx = {}
for entry in re.split(r"\n    SlashSpec \{", table)[1:]:
    name = re.search(r'name: "([^"]+)"', entry).group(1)
    aliases = re.findall(r'"([^"]+)"', re.search(r"aliases: &\[([^\]]*)\]", entry).group(1))
    has = re.search(r"has_args: (true|false)", entry).group(1) == "true"
    xfx[name] = (has, tuple(aliases))

def show(entry):
    has, aliases = entry
    return ("args" if has else "-") + (" " + " ".join(aliases) if aliases else "")

same = differ = 0
print(f"# fx: {len(fx)} commands (fx-src.txt M11); xfx at {rev}: {len(xfx)} (SLASH_REGISTRY)")
for name in sorted(set(fx) | set(xfx)):
    if name in fx and name in xfx:
        verdict = "SAME" if fx[name] == xfx[name] else "DIFF"
        same += verdict == "SAME"
        differ += verdict == "DIFF"
        print(f"{verdict} {name}: fx {show(fx[name])}; xfx {show(xfx[name])}")
    elif name in fx:
        print(f"FX-ONLY {name}: {show(fx[name])}")
    else:
        print(f"XFX-ONLY {name}: {show(xfx[name])}")
print(f"# shared {same + differ}: {same} the same, {differ} different")
sys.exit(1 if differ else 0)
