"""Copies SECRETS-ARGMAX's logs and tools into the receipt: every fake key
(`sk-` and 20 or more `A`s, whole or cut short by a log's column limit)
replaced by <fake-key>, because a committed key shape would be a finding of
the very check; local paths redacted by scripts/fxcap/redact.py, which
refuses to write if one survives. Bytes that are not UTF-8 (the Linux names
carry 0xFF) become U+FFFD.

usage: publish.py <worktree> <scratch>
"""
import pathlib
import re
import sys

worktree = pathlib.Path(sys.argv[1])
scratch = pathlib.Path(sys.argv[2])
sys.dont_write_bytecode = True
sys.path.insert(0, str(worktree / "scripts/fxcap"))
from redact import Redactor  # noqa: E402

out = worktree / ".prd/tui-phase5/receipts/secrets-argmax"
fake = re.compile(r"sk-A{20,}")
redactor = Redactor({
    "<tmp>": "<tmp>",
    str(scratch): "<scratch>",
    "<local-path>": "<copy>",
})

files = {
    "platform-macos.txt": scratch / "platform-macos.log",
    "platform-linux.txt": scratch / "linux-r2.log",
    "mutants.txt": scratch / "mutants-r2.out",
    "mutants.py": scratch / "mutants.py",
    "platform.sh": scratch / "tools/platform.sh",
    "gates.sh": scratch / "tools/gates.sh",
}
if len(sys.argv) > 3:
    files = {"gates.txt": scratch / "gates.log"}

for name, source in files.items():
    text = source.read_bytes().decode("utf-8", errors="replace")
    if name == "mutants.txt":
        # The driver's first line was printed by the session's shell, whose
        # `grep` is a wrapper; the tests run under bash, where `grep` is
        # /usr/bin/grep. Say which, rather than leave the wrapper's name.
        first, rest = text.split("\n", 1)
        bsd = "grep (BSD grep, GNU compatible) 2.6.0-FreeBSD"
        first = re.sub(r"; ugrep [^;]*(;.*)?$", f"; the tests' grep: /usr/bin/grep, {bsd}", first)
        text = first + "\n" + rest
    text = fake.sub("<fake-key>", text)
    redactor.write(out / name, text)
    print(f"wrote {name} ({len(text)} chars)")
