#!/usr/bin/env python3
"""Mutants of TEST-ENV-ISOLATION's guard, run on a throwaway clone only.

    TMPDIR=<short> mutants.py <clone> <head>

<clone> is a `git clone --no-local` copy (never the worktree): each mutant
removes part of the guard, so the code it builds is the code that reads the
process. Every run goes through sentinel-run.sh, so the process it can read is
built from sentinels, never the developer's. For each mutant: reset the clone
to <head>, apply one textual edit, build the lib tests, run the named cases;
a mutant is KILLED when at least one of them fails. Every edit is checked
to apply before anything is built. Prints one line per
mutant and a total; exits non-zero if any survives.
"""
import json
import os
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent

# Every mutant runs the same cases: the whole `test_env` module and the TUI
# thread's carry case. KILLED means at least one of them failed.
CASES = [
    "test_env::",
    "tui::signals::tests::a_thread_started_unsignalled_is_in_the_process_of_the_case_that_started_it",
]

# (name, file, old, new)
MUTANTS = [
    (
        "guard-gone: process_env! var/var_os read std::env in tests",
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
    ),
    (
        "refusal-gone: test_env::var_os returns None outside a fake",
        "src/test_env.rs",
        "        None => refuse(who, name),\n    }\n}\n\n/// `std::env::var(name)`, as `who`",
        "        None => { let _ = who; None }\n    }\n}\n\n/// `std::env::var(name)`, as `who`",
    ),
    (
        "config-vars-unrouted: Environment::from_process reads its variables from std::env",
        "src/config.rs",
        'process_env!("config::Environment::from_process", var(key))',
        "std::env::var(key)",
    ),
    (
        "config-home-unrouted: Environment::from_process reads HOME from std::env",
        "src/config.rs",
        'process_env!("config::Environment::from_process", var_os("HOME"))',
        'std::env::var_os("HOME")',
    ),
    (
        "gateway-reader-unrouted: Endpoint::from_process reads std::env",
        "src/gateway/mod.rs",
        'process_env!("gateway::Endpoint::from_process", var(GATEWAY_URL_ENV))',
        "std::env::var(GATEWAY_URL_ENV)",
    ),
    (
        "git-reader-unrouted: GitEnvironment::from_process reads std::env",
        "src/permission/gitconfig.rs",
        'process_env!("permission::GitEnvironment::from_process", var_os(name))',
        "std::env::var_os(name)",
    ),
    (
        "home-to-git-unrouted: minimal_environment's HOME is the process's",
        "src/permission/authority.rs",
        'process_env!(\n        "permission::authority::minimal_environment",\n        child_var("HOME")\n    )',
        'std::env::var("HOME")',
    ),
    (
        "child-isolation-gone: process::command inherits the process",
        "src/process.rs",
        "    #[cfg(test)]\n    crate::test_env::isolate_child(&mut command);\n",
        "",
    ),
    (
        "carry-gone: spawn_unsignalled does not carry the fake process",
        "src/tui/signals.rs",
        "    #[cfg(test)]\n    let body = crate::test_env::carried(body);\n",
        "",
    ),
    (
        "mcp-fallback-back: with_environment falls back to std::env",
        "src/mcp/runtime.rs",
        "Box::new(move |name| overrides.get(name).cloned().or_else(|| process(name))),",
        "Box::new(move |name| { let _ = &process; overrides.get(name).cloned().or_else(|| std::env::var(name).ok()) }),",
    ),
    (
        "scan-blind: a new direct read of HOME is not caught",
        "src/tui/term.rs",
        'std::env::var_os("TMUX").is_some_and(|value| !value.is_empty())',
        'std::env::var_os("TMUX").is_some_and(|value| !value.is_empty()) && std::env::var_os("HOME").is_some()',
    ),
]


def run(cmd, cwd, **kw):
    return subprocess.run(cmd, cwd=cwd, text=True, capture_output=True, **kw)


def lib_test_binary(clone):
    out = run(["cargo", "test", "--locked", "--lib", "--no-run", "--message-format=json"], clone)
    if out.returncode != 0:
        return None, out.stderr[-2000:]
    for line in out.stdout.splitlines():
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        if (msg.get("reason") == "compiler-artifact" and msg.get("executable")
                and msg["target"]["kind"] == ["lib"]):
            return msg["executable"], ""
    return None, "no lib test binary"


def main():
    clone = pathlib.Path(sys.argv[1]).resolve()
    head = sys.argv[2]
    assert (clone / ".git").is_dir(), "a clone, not a worktree"
    assert run(["git", "checkout", "-q", "--force", head], clone).returncode == 0
    for name, path, old, _ in MUTANTS:
        assert (clone / path).read_text().count(old) == 1, f"{name}: the edit does not apply"
    survived = 0
    for name, path, old, new in MUTANTS:
        assert run(["git", "checkout", "-q", "--force", head], clone).returncode == 0
        target = clone / path
        text = target.read_text()
        assert text.count(old) == 1, f"{name}: the edit does not apply"
        target.write_text(text.replace(old, new))
        binary, why = lib_test_binary(clone)
        if binary is None:
            print(f"KILLED (does not build) {name}")
            continue
        result = run([str(HERE / "sentinel-run.sh"), binary, *CASES], clone)
        failed = "test result: FAILED" in result.stdout
        ran = "running 0 tests" not in result.stdout
        verdict = "KILLED" if failed and ran else "SURVIVED"
        if verdict == "SURVIVED":
            survived += 1
        summary = [l for l in result.stdout.splitlines() if l.startswith("test result")]
        print(f"{verdict} {name} :: {summary[-1] if summary else 'no result'}")
    run(["git", "checkout", "-q", "--force", head], clone)
    print(f"total {len(MUTANTS)}, survived {survived}")
    sys.exit(1 if survived else 0)


if __name__ == "__main__":
    main()
