#!/usr/bin/env python3
"""RED and GREEN for the round-8 reviews' cfg_select! cases (round 9), on a
throwaway clone only.

    TMPDIR=<short> red.py <clone> <old rev> <new rev>

Each case is planted at <old rev> and at <new rev>, and that revision's own
gates run: `cargo clippy --all-targets -D warnings` and
`scripts/check-env-reads.py`. Each gate's verdict is printed, and when it
refuses, its first reasons (names and files, never a value).

The bypass's case is also built as a unit test, listed (`--list`) and run
alone under `../sentinel-run.sh`. It passes only if the arm clippy never
compiles is the one the test build took: that arm reads HOME, here the
sentinel home, and returns 1.
"""
import json
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
SENTINEL_RUN = HERE.parent / "sentinel-run.sh"

UNIT = "src/test_env/tests.rs"
UNIT_ANCHOR = "/// A comparison of a value read from a (fake) process environment: on a"

BYPASS = "where_clippy_never_looks"
CASES = [
    ("MUST (r8-probe/d.rs): a comma-less arm after `if .. else`, then `not(clippy)` over a HOME read", (
        UNIT, UNIT_ANCHOR,
        f"#[test]\nfn {BYPASS}() {{\n"
        "    let n: usize = cfg_select! { windows => if true { 0 } else { 0 } "
        'not(clippy) => std::env::var_os("HOME").map_or(0, |_| 1), _ => 7 };\n'
        '    assert_eq!(n, 1, "the arm clippy never compiles");\n}\n\n' + UNIT_ANCHOR), True),
    ("MUST (r8-probe/b.rs): a generic comma in an arm, a clippy lint allowed inside it", (
        UNIT, UNIT_ANCHOR,
        "#[test]\nfn a_generic_comma_in_an_arm() {\n"
        "    let x: Result<u8, u16> = cfg_select! { unix => Result::<u8, u16>::Ok({ "
        "#[allow(clippy::needless_return)] fn one() -> u8 { return 1; } one() }), _ => Ok(0) };\n"
        "    assert_eq!(x, Ok(1));\n}\n\n" + UNIT_ANCHOR), False),
]


def run(cmd, cwd, **kw):
    return subprocess.run(cmd, cwd=cwd, text=True, capture_output=True, **kw)


def reset(clone, rev):
    assert run(["git", "checkout", "-q", "--force", rev], clone).returncode == 0
    run(["git", "clean", "-q", "-fd", "--", "tests", "src"], clone)


def plant(clone, edit):
    path, old, new = edit
    target = clone / path
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


def built_and_taken(clone):
    """The bypass's case in the lib's unit-test build: listed, then run
    alone under the sentinel environment."""
    build = run(["cargo", "test", "--locked", "--lib", "--no-run", "--message-format=json"], clone)
    exe = None
    for line in build.stdout.splitlines():
        message = json.loads(line) if line.startswith("{") else {}
        if message.get("reason") == "compiler-artifact" and message.get("executable") and message["target"]["kind"] == ["lib"]:
            exe = message["executable"]
    if exe is None:
        return "    the unit-test build failed"
    listed = run([str(SENTINEL_RUN), exe, "--list"], clone).stdout
    names = [line.split(": ")[0] for line in listed.splitlines() if line.endswith(": test") and BYPASS in line]
    if not names:
        return f"    --list: {BYPASS} not listed"
    ran = run([str(SENTINEL_RUN), exe, "--exact", names[0]], clone)
    summary = [line for line in ran.stdout.splitlines() if line.startswith("test result")]
    return f"    --list: {names[0]}: test; run alone under sentinel-run.sh: {summary[-1] if summary else 'no result'}"


def main():
    clone = pathlib.Path(sys.argv[1]).resolve()
    old, new = sys.argv[2], sys.argv[3]
    assert (clone / ".git").is_dir(), "a clone, not a worktree"
    for case, edit, bypass in CASES:
        print(f"## {case}")
        for rev in (old, new):
            reset(clone, rev)
            plant(clone, edit)
            lint, result = gates(clone)
            print(say(rev, lint, result))
            if bypass:
                print(built_and_taken(clone))
    reset(clone, new)


if __name__ == "__main__":
    main()
