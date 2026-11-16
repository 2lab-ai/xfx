#!/usr/bin/env python3
"""RED for round 2's items 1 and 5, on a throwaway clone at the round-1 head.

    TMPDIR=<short> red-1-5.py <clone> <rev>

Item 1: three fixtures -- an imported `var`, an aliased `std::env`, a key
swap at an allowed site -- each applied alone; the round-1 scan
(`every_other_read_of_the_environment_in_src_is_listed`) is run. PASSED means
the fixture got past it: the RED.

Item 5: the guard's test arms routed back to `std::env` (the mutant
`guard-gone`); the `test_env` cases are run under sentinel-run.sh, and the
output is searched for the sentinel's values. FOUND means a failing case
printed a value of the process environment it ran in: the RED.

Every run goes through sentinel-run.sh: the process a case can read is
built from sentinels, never the developer's.
"""
import json
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
SENTINEL_RUN = HERE.parent / "sentinel-run.sh"
# What sentinel-run.sh sets: any of these in a case's output is a value of
# the process environment.
SENTINEL_VALUES = [
    "xfx-sentinel-model",
    "xfx-sentinel-gateway-key",
    "xfx-sentinel-oidc-token",
    "sentinel.invalid",
    "tei-sentinel",
]

FIXTURES = [
    (
        "imported var",
        "src/tui/term.rs",
        "pub(crate) fn",
        "pub(crate) fn fixture_imported() -> bool {\n    use std::env::var;\n    var(\"HOME\").is_ok()\n}\n\npub(crate) fn",
    ),
    (
        "aliased std::env",
        "src/tui/term.rs",
        "pub(crate) fn",
        "pub(crate) fn fixture_aliased() -> bool {\n    use std::env as process;\n    process::var_os(\"HOME\").is_some()\n}\n\npub(crate) fn",
    ),
    (
        "key swap at an allowed site",
        "src/tui/term.rs",
        'std::env::var_os("TMUX")',
        'std::env::var_os("HOME")',
    ),
]

GUARD_GONE = (
    "src/lib.rs",
    """    ($who:literal, var($name:expr)) => {
        $crate::test_env::var($who, $name)
    };
    ($who:literal, var_os($name:expr)) => {
        $crate::test_env::var_os($who, $name)
    };""",
    """    ($who:literal, var($name:expr)) => {
        ::std::env::var($name)
    };
    ($who:literal, var_os($name:expr)) => {
        ::std::env::var_os($name)
    };""",
)


def run(cmd, cwd):
    return subprocess.run(cmd, cwd=cwd, text=True, capture_output=True)


def checkout(clone, rev):
    assert run(["git", "checkout", "-q", "--force", rev], clone).returncode == 0


def edit(clone, path, old, new, first_only=False):
    target = clone / path
    text = target.read_text()
    assert text.count(old) >= 1, f"{path}: the edit does not apply"
    if not first_only:
        assert text.count(old) == 1, f"{path}: the edit is ambiguous"
    target.write_text(text.replace(old, new, 1))


def lib_tests(clone):
    out = run(["cargo", "test", "--locked", "--lib", "--no-run", "--message-format=json"], clone)
    for line in out.stdout.splitlines():
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        if (msg.get("reason") == "compiler-artifact" and msg.get("executable")
                and msg["target"]["kind"] == ["lib"]):
            return msg["executable"]
    sys.exit(f"no lib test binary: {out.stderr[-1500:]}")


def main():
    clone = pathlib.Path(sys.argv[1]).resolve()
    rev = sys.argv[2]
    assert (clone / ".git").is_dir(), "a clone, not a worktree"
    print(f"# at {rev}")
    for name, path, old, new in FIXTURES:
        checkout(clone, rev)
        edit(clone, path, old, new, first_only=True)
        result = run([str(SENTINEL_RUN), lib_tests(clone),
                      "test_env::tests::every_other_read_of_the_environment_in_src_is_listed"], clone)
        verdict = "PASSED (the fixture got past the scan)" if "test result: ok. 1 passed" in result.stdout \
            else "FAILED (the scan caught it)"
        print(f"item 1, {name}: round-1 scan {verdict}")
    checkout(clone, rev)
    edit(clone, *GUARD_GONE)
    result = run([str(SENTINEL_RUN), lib_tests(clone), "test_env::"], clone)
    output = result.stdout + result.stderr
    found = [value for value in SENTINEL_VALUES if value in output]
    summary = [line for line in result.stdout.splitlines() if line.startswith("test result")]
    print(f"item 5, guard-gone: {summary[-1] if summary else 'no result'}")
    print(f"item 5, sentinel values in the failing cases' output: "
          f"{'FOUND ' + str(len(found)) + ' of ' + str(len(SENTINEL_VALUES)) if found else 'none'}")
    for value in found:
        lines = sum(1 for line in output.splitlines() if value in line)
        print(f"  {value}: {lines} line(s)")
    checkout(clone, rev)


if __name__ == "__main__":
    main()
