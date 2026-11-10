#!/usr/bin/env python3
"""RED and GREEN for the round-4 reviews' holes (round 5), on a throwaway
clone only.

    TMPDIR=<short> red.py <clone> <old rev> <new rev> [title filter...]

For each hole and each revision: reset the clone, plant the hole, and run
the gates that revision has -- `cargo clippy --all-targets -D warnings` and
`scripts/check-env-reads.py` -- reporting each gate's verdict and, when it
refuses, its first reasons (names and files, never a value). At the old
revision every hole passes every gate; at the new one the check refuses
each.

Then the HOME holes, which are about what a binary does when run: at the
new revision, with the new tests/support/isolated.rs and with the old one
put in its place, the isolation test binary is started as tests/isolation.rs
starts it -- a marker, its HOME and TMPDIR, asked for the case that leaves
a mark where it runs -- once per hole, and each run reported: refused or
not, whether a case left its mark, and whether a sentinel value was printed.
"""
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
SENTINEL_RUN = HERE.parent / "sentinel-run.sh"
UNIT = "src/test_env/tests.rs"
UNIT_ANCHOR = "fn same(equal: bool, which: &str) {"

HOLES = [
    ("opus M1: an aliased Command in a src test module, its child given the developer's HOME", [(
        UNIT, UNIT_ANCHOR,
        "#[allow(dead_code)]\nfn a_git_with_the_real_home() {\n    use std::process::Command as Git;\n"
        '    let mut git = Git::new("git");\n    git.args(["config", "--global", "--list"]);\n'
        "    let _ = crate::process::output(&mut git);\n}\n\n"
        + UNIT_ANCHOR)]),
    ("gpt6 check MUST 1: option_env!(XFX_GATEWAY_KEY) in a src test", [(
        UNIT, UNIT_ANCHOR,
        '#[test]\nfn reads_a_key_at_compile_time() {\n    assert!(option_env!("XFX_GATEWAY_KEY").is_none());\n}\n\n'
        + UNIT_ANCHOR)]),
    ("gpt6 check MUST 2: a HOME read in a release-only test", [(
        UNIT, UNIT_ANCHOR,
        "#[cfg(not(debug_assertions))]\n#[test]\n#[allow(clippy::disallowed_methods)]\nfn a_release_only_read() {\n"
        '    let _ = std::env::var_os("HOME");\n}\n\n' + UNIT_ANCHOR)]),
    ("gpt6 isolation MUST 3: a test file whose support only clippy compiles", [(
        "tests/escape.rs", None,
        "#[cfg(clippy)]\nmod support;\n\n#[test]\nfn runs_in_the_developers_process() {}\n")]),
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


def gates(clone):
    lint = run(["cargo", "clippy", "--locked", "--all-targets", "--", "-D", "warnings"], clone)
    lint_why = sorted({line.strip() for line in lint.stderr.splitlines() if line.startswith("error")})
    check = run(["python3", "-I", "scripts/check-env-reads.py"], clone)
    check_why = [line.strip() for line in check.stdout.splitlines() if line.strip().startswith("refused:")]
    return (lint.returncode, lint_why[:2]), (check.returncode, check_why[:3])


def home_holes(clone, old, new):
    import os, shutil, tempfile
    reset(clone, old)
    old_copy = (clone / "tests/support/isolated.rs").read_text()
    marker = "0123456789abcdef0123456789abcdef"
    for rev in (old, new):
        reset(clone, new)
        if rev == old:
            (clone / "tests/support/isolated.rs").write_text(old_copy)
        build = run(["cargo", "test", "--locked", "--test", "isolation", "--no-run"], clone)
        binary = next((clone / line.split("(")[1].rstrip(")") for line in build.stderr.splitlines()
                       if "Executable tests/isolation.rs" in line), None)
        assert binary is not None, build.stderr[-2000:]
        print(f"  with {rev}'s isolated.rs:")
        for hole in ("a home with the marker's name in another directory",
                     "a link to an empty private directory at the exact path",
                     "that link, with a trailing /",
                     "a TMPDIR no home can be made under, its value a sentinel"):
            scratch = pathlib.Path(tempfile.mkdtemp(prefix="red-home-"))
            tmpdir = scratch
            target = scratch / "target"
            target.mkdir(mode=0o700)
            exact = scratch / f"xfx-test-home-{marker}"
            given_marker = marker
            if hole.startswith("a home with"):
                (scratch / "elsewhere").mkdir(mode=0o700)
                home = scratch / "elsewhere" / f"xfx-test-home-{marker}"
                home.mkdir(mode=0o700)
            elif hole.startswith("a link"):
                exact.symlink_to(target)
                home = exact
            elif hole.startswith("that link"):
                exact.symlink_to(target)
                home = exact
            else:
                tmpdir = scratch / "xfx-sentinel-tmpdir-value" / "gone"
                home = target
                given_marker = None
            # pathlib drops a trailing "/", so the variable is built as text.
            home_value = str(home) + ("/" if hole.startswith("that link") else "")
            env = {"PATH": os.environ["PATH"], "HOME": home_value, "TMPDIR": str(tmpdir)}
            if given_marker:
                env["XFX_TEST_ISOLATED_COPY"] = given_marker
            result = run([str(binary), "--ignored", "--exact", "a_case_marks_where_it_runs"],
                         home_value, env=env, timeout=120)
            said = result.stdout + result.stderr
            refused = "no case runs" in said
            ran = "test result: ok. 1 passed" in result.stdout
            marked = (home / "ran").exists() or (target / "ran").exists()
            print(f"    {hole}: {'refused' if refused else 'not refused'} (exit {result.returncode}); "
                  f"the case ran: {'yes' if ran else 'no'}; its mark left: {'yes' if marked else 'no'}; "
                  f"the home it was handed still there: {'yes' if home.exists() else 'no'}; "
                  f"what the link points at still there: {'yes' if target.exists() else 'no'}; "
                  f"sentinel printed: {'yes' if 'xfx-sentinel-tmpdir-value' in said else 'no'}")
            shutil.rmtree(scratch, ignore_errors=True)
    reset(clone, new)


def main():
    clone = pathlib.Path(sys.argv[1]).resolve()
    old, new = sys.argv[2], sys.argv[3]
    only = sys.argv[4:]
    assert (clone / ".git").is_dir(), "a clone, not a worktree"
    for hole, edits in HOLES:
        if only and not any(word in hole for word in only):
            continue
        print(f"## {hole}")
        for rev in (old, new):
            reset(clone, rev)
            plant(clone, edits)
            (lint, lint_why), (check, check_why) = gates(clone)
            print(f"  at {rev}: clippy -D warnings {'passes' if lint == 0 else 'refuses'}"
                  + (f" ({'; '.join(lint_why)})" if lint else "")
                  + f"; check-env-reads {'passes' if check == 0 else 'refuses'}"
                  + (f" ({' | '.join(check_why)})" if check else ""))
    if not only or "HOME" in only:
        print("## gpt6 isolation MUSTs 1-2, opus M3: HOME, run against each revision's isolated.rs")
        home_holes(clone, old, new)
    reset(clone, new)


if __name__ == "__main__":
    main()
