#!/usr/bin/env python3
"""Which unit tests build a client that may take a proxy (item 6).

    TMPDIR=<short> https-clients.py <clone> <rev>

On a throwaway clone at <rev>, `bearer_rules` is made to print one marker
line naming the thread (libtest names it after the case) whenever it is
given an https URL -- the one branch where a build keeps reqwest's proxy
reading. The whole lib suite then runs under sentinel-run.sh with
--nocapture and --test-threads=1, and the cases that printed the marker are
listed. Reads code and markers only.
"""
import json
import pathlib
import re
import subprocess
import sys
from collections import Counter

HERE = pathlib.Path(__file__).resolve().parent
SENTINEL_RUN = HERE.parent / "sentinel-run.sh"
OLD = "    if may_use_a_proxy(url) {\n        builder\n"
NEW = ("    if may_use_a_proxy(url) {\n"
       "        eprintln!(\"XFX-HTTPS-CLIENT {}\", std::thread::current().name().unwrap_or(\"?\"));\n"
       "        builder\n")


def run(cmd, cwd):
    return subprocess.run(cmd, cwd=cwd, text=True, capture_output=True)


def main():
    clone = pathlib.Path(sys.argv[1]).resolve()
    rev = sys.argv[2]
    assert (clone / ".git").is_dir(), "a clone, not a worktree"
    assert run(["git", "checkout", "-q", "--force", rev], clone).returncode == 0
    target = clone / "src/gateway/mod.rs"
    text = target.read_text()
    assert text.count(OLD) == 1, "bearer_rules moved"
    target.write_text(text.replace(OLD, NEW))
    out = run(["cargo", "test", "--locked", "--lib", "--no-run", "--message-format=json"], clone)
    binary = next(json.loads(line)["executable"] for line in out.stdout.splitlines()
                  if line.startswith("{") and json.loads(line).get("reason") == "compiler-artifact"
                  and json.loads(line).get("executable")
                  and json.loads(line)["target"]["kind"] == ["lib"])
    result = run([str(SENTINEL_RUN), binary, "--nocapture", "--test-threads=1"], clone)
    output = result.stdout + result.stderr
    names = Counter(m.group(1) for m in re.finditer(r"XFX-HTTPS-CLIENT (\S+)", output))
    summary = [line for line in result.stdout.splitlines() if line.startswith("test result")]
    print(f"# at {rev}: {summary[-1] if summary else 'no result'}")
    print(f"# threads that built an https bearer client: {len(names)}, clients: {sum(names.values())}")
    for name, count in sorted(names.items()):
        print(f"{count:4d} {name}")
    run(["git", "checkout", "-q", "--force", rev], clone)


if __name__ == "__main__":
    main()
