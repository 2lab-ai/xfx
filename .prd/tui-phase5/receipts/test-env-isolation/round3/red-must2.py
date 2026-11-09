#!/usr/bin/env python3
"""RED for round 3's MUST 2: each way round round 2's binding check that the
reviews named, applied alone to a throwaway clone at <rev>, then round 2's
own checks run under sentinel-run.sh.

    TMPDIR=<short> red-must2.py <clone> <rev>

PASSED means the bypass got past them: the RED. Each fixture reads, or
starts, something an allowed unit was not bound to: the whole environment,
`HOME` by a name held elsewhere, `HOME` by a name the check cannot see,
and a child through an aliased `Command`.
"""
import json
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
SENTINEL_RUN = HERE.parent / "sentinel-run.sh"
CHECKS = [
    "test_env::tests::every_read_outside_process_env_is_bound_to_its_variables",
    "process::tests::no_child_process_is_made_any_other_way",
]
TMUX = 'std::env::var_os("TMUX").is_some_and(|value| !value.is_empty())'
FIXTURES = [
    ("enumeration inside an allowed function",
     [("src/tui/term.rs", TMUX,
       '(std::env::vars_os().count() > 0, ' + TMUX + ').1')]),
    ("an unbound key: the name held in a variable outside the allow",
     [("src/tui/mod.rs",
       "    #[allow(\n        clippy::disallowed_methods,\n        reason = \"env: COLORFGBG -- what the terminal says of itself\"\n    )]\n    let colorfgbg = std::env::var(theme::COLORFGBG).ok();",
       "    let key = \"HOME\";\n    #[allow(\n        clippy::disallowed_methods,\n        reason = \"env: COLORFGBG -- what the terminal says of itself\"\n    )]\n    let colorfgbg = std::env::var(key).ok().or(Some(theme::COLORFGBG.to_string()));")]),
    ("a function-level allow over a read the check cannot name",
     [("src/mcp/docker.rs",
       '    ["TMPDIR", "TEMP", "TMP"]\n        .iter()',
       '    let _ = std::env::var_os("home".to_uppercase());\n    ["TMPDIR", "TEMP", "TMP"]\n        .iter()')]),
    ("an aliased Command started with .output() inside an allow",
     [("src/tui/term.rs", "#[allow(\n    clippy::disallowed_methods,\n    reason = \"env: TMUX -- whether the terminal is tmux's\"\n)]\npub(crate) fn under_tmux() -> bool {\n    " + TMUX,
       "use std::process::Command as C;\n\n#[allow(\n    clippy::disallowed_methods,\n    reason = \"env: TMUX -- whether the terminal is tmux's\"\n)]\npub(crate) fn under_tmux() -> bool {\n    let _ = C::new(\"env\").output();\n    " + TMUX)]),
]


def run(cmd, cwd):
    return subprocess.run(cmd, cwd=cwd, text=True, capture_output=True)


def main():
    clone = pathlib.Path(sys.argv[1]).resolve()
    rev = sys.argv[2]
    assert (clone / ".git").is_dir(), "a clone, not a worktree"
    print(f"# at {rev}: round 2's checks after each bypass")
    for name, edits in FIXTURES:
        assert run(["git", "checkout", "-q", "--force", rev], clone).returncode == 0
        for path, old, new in edits:
            target = clone / path
            text = target.read_text()
            assert text.count(old) == 1, f"{name}: does not apply"
            target.write_text(text.replace(old, new))
        out = run(["cargo", "test", "--locked", "--lib", "--no-run", "--message-format=json"], clone)
        binary = next((json.loads(l)["executable"] for l in out.stdout.splitlines()
                       if l.startswith("{") and json.loads(l).get("reason") == "compiler-artifact"
                       and json.loads(l).get("executable") and json.loads(l)["target"]["kind"] == ["lib"]), None)
        if binary is None:
            print(f"DOES NOT BUILD {name}")
            continue
        result = run([str(SENTINEL_RUN), binary, *CHECKS], clone)
        summary = [l for l in result.stdout.splitlines() if l.startswith("test result")]
        passed = "test result: ok. 2 passed" in result.stdout
        print(f"{'PASSED (the bypass got past them)' if passed else 'FAILED (caught)'} {name} :: {summary[-1] if summary else 'no result'}")
    run(["git", "checkout", "-q", "--force", rev], clone)


if __name__ == "__main__":
    main()
