#!/usr/bin/env python3
"""RED and GREEN for the round-4 reviews' holes (round 5), on a throwaway
clone only.

    TMPDIR=<short> red.py <clone> <old rev> <new rev>

For each hole and each revision: reset the clone, plant the hole, and run
the gates that revision has -- `cargo clippy --all-targets -D warnings` and
`scripts/check-env-reads.py` -- reporting each gate's verdict and, when it
refuses, its first reasons (names and files, never a value). At the old
revision every hole passes every gate; at the new one the check refuses
each.

Then the HOME holes, which are about what a binary does when run: at the
new revision, tests/isolation.rs is run under sentinel-run.sh against the
new tests/support/isolated.rs and against the old one put in its place, and
each of its cases reported.
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
        '    let _ = crate::process::output(&mut Git::new("git").args(["config", "--global", "--list"]));\n}\n\n'
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


def main():
    clone = pathlib.Path(sys.argv[1]).resolve()
    old, new = sys.argv[2], sys.argv[3]
    assert (clone / ".git").is_dir(), "a clone, not a worktree"
    for hole, edits in HOLES:
        print(f"## {hole}")
        for rev in (old, new):
            reset(clone, rev)
            plant(clone, edits)
            (lint, lint_why), (check, check_why) = gates(clone)
            print(f"  at {rev}: clippy -D warnings {'passes' if lint == 0 else 'refuses'}"
                  + (f" ({'; '.join(lint_why)})" if lint else "")
                  + f"; check-env-reads {'passes' if check == 0 else 'refuses'}"
                  + (f" ({' | '.join(check_why)})" if check else ""))
    # The HOME holes: the new regressions against each revision's copy.
    reset(clone, old)
    old_copy = (clone / "tests/support/isolated.rs").read_text()
    print("## gpt6 isolation MUSTs 1-2, opus M3: the new tests/isolation.rs against each revision's isolated.rs")
    for rev in (old, new):
        reset(clone, new)
        if rev == old:
            (clone / "tests/support/isolated.rs").write_text(old_copy)
        build = run(["cargo", "test", "--locked", "--test", "isolation", "--no-run"], clone)
        binary = next((clone / line.split("(")[1].rstrip(")") for line in build.stderr.splitlines()
                       if "Executable tests/isolation.rs" in line), None)
        if binary is None:
            print(f"  at {rev}: does not build: " + "; ".join(sorted({l.strip() for l in build.stderr.splitlines() if l.startswith("error")})[:2]))
            continue
        result = run([str(SENTINEL_RUN), str(binary), "--test-threads=1"], clone, timeout=600)
        cases = [line for line in result.stdout.splitlines() if line.startswith("test ") and " ... " in line]
        panics = [line.strip() for line in result.stdout.splitlines()
                  if "a case ran" in line or "printed TMPDIR" in line or "did not refuse" in line]
        print(f"  at {rev}: " + "; ".join(cases))
        for line in panics[:4]:
            # a panic line names the home it was handed, a scratch path: cut it
            print("    " + line.split(" under ")[0].split(" (")[0][:160])
    reset(clone, new)


if __name__ == "__main__":
    main()
