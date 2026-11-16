#!/usr/bin/env python3
"""RED and GREEN for the round-7 review's hole (round 8), on a throwaway
clone only.

    TMPDIR=<short> red.py <clone> <old rev> <new rev>

Each hole is planted at <old rev> and at <new rev>, and that revision's own
gates run: `cargo clippy --all-targets -D warnings` and
`scripts/check-env-reads.py`. Each gate's verdict is printed, and when it
refuses, its first reasons (names and files, never a value).
"""
import pathlib
import subprocess
import sys

UNIT = "src/test_env/tests.rs"
UNIT_ANCHOR = "/// A comparison of a value read from a (fake) process environment: on a"

READ_HOME = '    let _ = std::env::var_os("HOME");\n'

HOLES = [
    ("MUST: opus's cfg_select!, its clippy arm empty, a HOME read in the other", [(
        UNIT, UNIT_ANCHOR,
        "cfg_select! {\n    clippy => {}\n    _ => {\n        #[test]\n        fn where_clippy_never_looks() {\n"
        '            let _ = std::env::var_os("HOME");\n        }\n    }\n}\n\n' + UNIT_ANCHOR)]),
]

def run(cmd, cwd, **kw):
    return subprocess.run(cmd, cwd=cwd, text=True, capture_output=True, **kw)


def reset(clone, rev):
    assert run(["git", "checkout", "-q", "--force", rev], clone).returncode == 0
    run(["git", "clean", "-q", "-fd", "--", "tests", "src"], clone)


def plant(clone, edits):
    for path, old, new in edits:
        target = clone / path
        if old is None:
            target.write_text(new)
        else:
            text = target.read_text()
            assert text.count(old) == 1, path
            target.write_text(text.replace(old, new))


def check(clone):
    out = run(["python3", "-I", "scripts/check-env-reads.py"], clone)
    why = [line.strip() for line in out.stdout.splitlines() if line.strip().startswith("refused:")]
    return out.returncode, why[:3]


def gates(clone):
    lint = run(["cargo", "clippy", "--locked", "--all-targets", "--", "-D", "warnings"], clone)
    lint_why = sorted({line.strip() for line in lint.stderr.splitlines() if line.startswith("error")})
    return (lint.returncode, lint_why[:2]), check(clone)


def say(rev, lint, check_result):
    (status, why), (check_status, check_why) = lint, check_result
    return (f"  at {rev}: clippy -D warnings {'passes' if status == 0 else 'refuses'}"
            + (f" ({'; '.join(why)})" if status else "")
            + f"; check-env-reads {'passes' if check_status == 0 else 'refuses'}"
            + (f" ({' | '.join(check_why)})" if check_status else ""))


def main():
    clone = pathlib.Path(sys.argv[1]).resolve()
    old, new = sys.argv[2], sys.argv[3]
    assert (clone / ".git").is_dir(), "a clone, not a worktree"
    for hole, edits in HOLES:
        print(f"## {hole}")
        for rev in (old, new):
            reset(clone, rev)
            plant(clone, edits)
            lint, result = gates(clone)
            print(say(rev, lint, result))
    reset(clone, new)


if __name__ == "__main__":
    main()
