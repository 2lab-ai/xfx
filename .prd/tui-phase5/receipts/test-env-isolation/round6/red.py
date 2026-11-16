#!/usr/bin/env python3
"""RED and GREEN for the round-5 reviews' holes (round 6), on a throwaway
clone only.

    TMPDIR=<short> red.py <clone> <old rev> <new rev>

Each hole is planted at <old rev> and at <new rev>, and that revision's own
gates run: `cargo clippy --all-targets -D warnings` and
`scripts/check-env-reads.py`. Each gate's verdict is printed, and when it
refuses, its first reasons (names and files, never a value).

The missing dep-info hole needs a build first: the check runs once, then
every dep-info file of the xfx library is removed -- cargo calls the builds
fresh and does not write them again -- and the check runs once more. The
clone's xfx artifacts are cleaned afterwards (`cargo clean -p xfx`), so the
next user rebuilds what is gone.

The relative TMPDIR hole is about what the isolation test binary does: it
is started from a scratch directory with `TMPDIR=relative`, with each
revision's tests/support/isolated.rs, and what happened is printed.
"""
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

UNIT = "src/test_env/tests.rs"
UNIT_ANCHOR = "/// A comparison of a value read from a (fake) process environment: on a"

HOLES = [
    ("MUST 1: a HOME read and a git start in a unit test under cfg(all(test, not(clippy)))", [(
        UNIT, UNIT_ANCHOR,
        "#[cfg(all(test, not(clippy)))]\n#[test]\nfn where_clippy_never_looks() {\n"
        '    let _ = std::env::var_os("HOME");\n'
        '    let _ = std::process::Command::new("git").args(["config", "--global", "--list"]);\n}\n\n'
        + UNIT_ANCHOR)]),
    ("MUST 2: a raw syscall in a unit test", [(
        UNIT, UNIT_ANCHOR,
        "#[test]\nfn a_raw_system_call() {\n    // SAFETY: getpid takes nothing.\n"
        "    let _ = unsafe { libc::syscall(20) };\n}\n\n" + UNIT_ANCHOR)]),
    ("MUST 2: execvP (Apple) in a unit test", [(
        UNIT, UNIT_ANCHOR,
        '#[cfg(target_vendor = "apple")]\n#[allow(dead_code)]\nfn replace_the_process() {\n'
        "    // SAFETY: never called.\n"
        "    let _ = unsafe { libc::execvP(std::ptr::null(), std::ptr::null(), std::ptr::null()) };\n}\n\n"
        + UNIT_ANCHOR)]),
    ("SHOULD: the home from the user database in a unit test", [(
        UNIT, UNIT_ANCHOR,
        "#[test]\nfn the_home_from_the_user_database() {\n    // SAFETY: getuid cannot fail.\n"
        "    let _ = unsafe { libc::getpwuid(libc::getuid()) };\n}\n\n" + UNIT_ANCHOR)]),
    ("SHOULD: support compiled in every build but release with all features", [(
        "tests/escape.rs", None,
        '#[cfg(any(debug_assertions, not(feature = "fault-injection")))]\nmod support;\n\n'
        "#[test]\nfn runs_in_the_developers_process() {}\n")]),
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


def remove_dep_info(clone):
    """Every dep-info file of the xfx library's own compilations."""
    removed = 0
    for profile in ("debug", "release"):
        deps = clone / "target" / profile / "deps"
        for path in deps.glob("xfx-*.d") if deps.is_dir() else ():
            path.unlink()
            removed += 1
    return removed


def relative_tmpdir(clone, old, new):
    reset(clone, old)
    old_copy = (clone / "tests/support/isolated.rs").read_text()
    for rev in (old, new):
        reset(clone, new)
        if rev == old:
            (clone / "tests/support/isolated.rs").write_text(old_copy)
        build = run(["cargo", "test", "--locked", "--test", "isolation", "--no-run"], clone)
        binary = next((clone / line.split("(")[1].rstrip(")") for line in build.stderr.splitlines()
                       if "Executable tests/isolation.rs" in line), None)
        assert binary is not None, build.stderr[-2000:]
        scratch = pathlib.Path(tempfile.mkdtemp(prefix="red-rel-"))
        (scratch / "relative").mkdir(mode=0o700)
        env = {"PATH": os.environ["PATH"], "HOME": str(scratch), "TMPDIR": "relative"}
        result = run([str(binary), "--ignored", "--exact", "a_case_marks_where_it_runs"], str(scratch),
                     env=env, timeout=120)
        said = result.stdout + result.stderr
        made = [p.name for p in (scratch / "relative").iterdir()]
        print(f"  with {rev}'s isolated.rs: {'refused' if 'no case runs' in said else 'not refused'} "
              f"(exit {result.returncode}); the case ran: {'yes' if 'test result: ok. 1 passed' in result.stdout else 'no'}; "
              f"a home was made under the relative TMPDIR: {'yes' if made else 'no'}; "
              f"refusal: {next((l.split('no case runs: ', 1)[1] for l in result.stderr.splitlines() if 'no case runs: ' in l), '-')}")
        shutil.rmtree(scratch, ignore_errors=True)
    reset(clone, new)


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
    print("## MUST 3: a nested env! whose dep-info is gone (the check run once to build, the .d files removed, run again)")
    for rev in (old, new):
        reset(clone, rev)
        plant(clone, [NESTED])
        first = check(clone)
        removed = remove_dep_info(clone)
        second = check(clone)
        run(["cargo", "clean", "-p", "xfx"], clone)
        print(f"  at {rev}: with its dep-info, check-env-reads {'passes' if first[0] == 0 else 'refuses'}"
              + (f" ({' | '.join(first[1])})" if first[0] else "")
              + f"; {removed} .d files removed; then check-env-reads {'passes' if second[0] == 0 else 'refuses'}"
              + (f" ({' | '.join(second[1])})" if second[0] else ""))
    print("## SHOULD: a relative TMPDIR, the isolation binary started with each revision's isolated.rs")
    relative_tmpdir(clone, old, new)
    # The removed dep-info is rebuilt for whoever uses the clone next.
    reset(clone, new)
    run(["cargo", "clean", "-p", "xfx"], clone)


if __name__ == "__main__":
    main()
