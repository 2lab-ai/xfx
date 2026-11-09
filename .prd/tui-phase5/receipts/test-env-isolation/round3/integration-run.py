#!/usr/bin/env python3
"""Every integration suite at a revision, under sentinel-run.sh, on a
throwaway clone: each suite's result and its failing cases with the first
line of each panic (round 3, MUST 1's RED and GREEN).

    TMPDIR=<short> integration-run.py <clone> <rev> [suite...]

sentinel-run.sh's HOME holds a `.gitconfig` and an XDG git config that each
define a hook, so a case that plans or runs git in its own process under
that HOME is decided by them. Output names cases and assertion lines; a line
that names a scratch path has it replaced by <tmp>.
"""
import pathlib
import re
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
SENTINEL_RUN = HERE.parent / "sentinel-run.sh"


def main():
    clone = pathlib.Path(sys.argv[1]).resolve()
    rev = sys.argv[2]
    only = sys.argv[3:]
    assert (clone / ".git").is_dir(), "a clone, not a worktree"
    assert subprocess.run(["git", "checkout", "-q", "--force", rev], cwd=clone).returncode == 0
    build = subprocess.run(["cargo", "test", "--locked", "--tests", "--no-run"], cwd=clone,
                           text=True, capture_output=True)
    assert build.returncode == 0, build.stderr[-3000:]
    binaries = {}
    for line in build.stderr.splitlines():
        m = re.search(r"Executable tests/(\w+)\.rs \((.*)\)", line)
        if m:
            binaries[m.group(1)] = clone / m.group(2)
    print(f"# at {rev}: every integration suite under sentinel-run.sh")
    for suite in sorted(binaries):
        if only and suite not in only:
            continue
        out = subprocess.run([str(SENTINEL_RUN), str(binaries[suite]), "--test-threads=4"],
                             cwd=clone, text=True, capture_output=True)
        text = out.stdout + out.stderr
        summary = [l for l in out.stdout.splitlines() if l.startswith("test result")]
        print(f"suite {suite}: {summary[-1] if summary else 'no result'}")
        for m in re.finditer(r"---- (\S+) stdout ----\n(.*?)(?=\n---- \S+ stdout ----\n|\nfailures:\n)", text, re.S):
            lines = [l for l in m.group(2).splitlines() if l.strip()]
            panic = next((i for i, l in enumerate(lines) if "panicked at" in l), None)
            said = lines[panic + 1] if panic is not None and panic + 1 < len(lines) else "?"
            # The scratch roots, spelled in parts so that this file names none.
            roots = "|".join([r"/(private/)?tmp/claude-[0-9]+", "/var/" + "folders/", "/" + "Users/"])
            said = re.sub(rf"(?:{roots})[^\s\"'`]*", "<tmp>", said)
            print(f"  FAILED {m.group(1)} :: {said[:200]}")


if __name__ == "__main__":
    main()
