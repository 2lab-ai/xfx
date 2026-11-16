#!/usr/bin/env python3
"""RED and GREEN for the round-6 reviews' holes (round 7), on a throwaway
clone only.

    TMPDIR=<short> red.py <clone> <old rev> <new rev>

Each hole is planted at <old rev> and at <new rev>, and that revision's own
gates run: `cargo clippy --all-targets -D warnings` and
`scripts/check-env-reads.py`. Each gate's verdict is printed, and when it
refuses, its first reasons (names and files, never a value).

The emptied dep-info hole needs a build first: the check runs once, then
every dep-info file of the xfx library is emptied -- cargo calls the builds
fresh and does not write them again -- and the check runs once more. The
clone's xfx artifacts are cleaned afterwards (`cargo clean -p xfx`), so the
next user rebuilds what is gone.
"""
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

UNIT = "src/test_env/tests.rs"
UNIT_ANCHOR = "/// A comparison of a value read from a (fake) process environment: on a"

READ_HOME = '    let _ = std::env::var_os("HOME");\n'

HOLES = [
    ("MUST 1(a): #[cfg(/*)*/ all(test, not(clippy)))] over a HOME read in a unit test", [(
        UNIT, UNIT_ANCHOR,
        "#[cfg(/*)*/ all(test, not(clippy)))]\n#[test]\nfn behind_a_comment() {\n" + READ_HOME + "}\n\n"
        + UNIT_ANCHOR)]),
    ("MUST 1(a): #[cfg /* c */ (not(clippy))] over a HOME read in a unit test", [(
        UNIT, UNIT_ANCHOR,
        "#[cfg /* c */ (not(clippy))]\n#[test]\nfn a_comment_before_the_paren() {\n" + READ_HOME + "}\n\n"
        + UNIT_ANCHOR)]),
    ("MUST 1(b): a block comment closed on a line that starts with //, then the cfg", [(
        UNIT, UNIT_ANCHOR,
        "/*\n// */ #[cfg(all(test, not(clippy)))]\n#[test]\nfn after_a_block_comment() {\n" + READ_HOME + "}\n\n"
        + UNIT_ANCHOR)]),
    ("MUST 1(c): a #[path] module file that is not .rs, its test under not(clippy)", [
        (UNIT, UNIT_ANCHOR, '#[path = "hidden.inc"]\nmod hidden;\n\n' + UNIT_ANCHOR),
        ("src/test_env/hidden.inc", None,
         "#[cfg(not(clippy))]\n#[test]\nfn in_a_path_module() {\n" + READ_HOME + "}\n")]),
    ("SHOULD: build.rs prints a rustc-cfg", [(
        "build.rs", '    println!("cargo:rerun-if-changed=build.rs");',
        '    println!("cargo:rerun-if-changed=build.rs");\n    println!("cargo:rustc-cfg=xfx_hidden");')]),
    ("SHOULD: getpwent in a unit test", [(
        UNIT, UNIT_ANCHOR,
        "#[test]\nfn the_homes_of_every_user() {\n    // SAFETY: getpwent takes nothing.\n"
        "    let _ = unsafe { libc::getpwent() };\n}\n\n" + UNIT_ANCHOR)]),
    ("SHOULD: a default feature", [(
        "Cargo.toml", "fault-injection = []\n", "default = []\nfault-injection = []\n")]),
]

# A compile-time read clippy's macro lint does not see (nested in concat!),
# whose only record is rustc's dep-info -- then the dep-info removed.
NESTED = (UNIT, UNIT_ANCHOR,
          '#[allow(dead_code)]\nconst BY: &str = concat!("by ", env!("CARGO_PKG_AUTHORS"));\n\n' + UNIT_ANCHOR)


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


def empty_dep_info(clone):
    """Every dep-info file of the xfx library's own compilations, emptied."""
    emptied = 0
    for profile in ("debug", "release"):
        deps = clone / "target" / profile / "deps"
        for path in deps.glob("xfx-*.d") if deps.is_dir() else ():
            path.write_bytes(b"")
            emptied += 1
    return emptied


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
    print("## MUST 2: a nested env! whose dep-info is emptied (the check run once to build, the .d files emptied, run again)")
    for rev in (old, new):
        reset(clone, rev)
        plant(clone, [NESTED])
        first = check(clone)
        removed = empty_dep_info(clone)
        second = check(clone)
        run(["cargo", "clean", "-p", "xfx"], clone)
        print(f"  at {rev}: with its dep-info, check-env-reads {'passes' if first[0] == 0 else 'refuses'}"
              + (f" ({' | '.join(first[1])})" if first[0] else "")
              + f"; {removed} .d files emptied; then check-env-reads {'passes' if second[0] == 0 else 'refuses'}"
              + (f" ({' | '.join(second[1])})" if second[0] else ""))
    # The removed dep-info is rebuilt for whoever uses the clone next.
    reset(clone, new)
    run(["cargo", "clean", "-p", "xfx"], clone)


if __name__ == "__main__":
    main()
