#!/usr/bin/env python3
"""Round 2's mutants and fixtures, on a throwaway clone only.

    TMPDIR=<short> mutants-r2.py <clone> <head>

<clone> is a `git clone --no-local` copy, never the worktree. For each entry:
reset the clone to <head>, apply one textual edit, then

- a mutant (kind "test"): build the lib tests and run CASES under
  sentinel-run.sh; KILLED when a case fails;
- a fixture (kind "clippy"): run clippy on every target with -D warnings;
  KILLED when it fails on a disallowed method;
- a fixture (kind "check"): build the lib tests and run the binding check;
  KILLED when it fails.

Every test run is also searched for sentinel-run.sh's values: a failing case
must say which variable differs, never what it holds (item 5). The values
are counted, not printed. Exits non-zero if anything survives or leaks.
"""
import json
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
SENTINEL_RUN = HERE.parent / "sentinel-run.sh"
SENTINEL_VALUES = [
    "xfx-sentinel-model",
    "xfx-sentinel-gateway-key",
    "xfx-sentinel-oidc-token",
    "sentinel.invalid",
    "tei-sentinel",
]
CASES = [
    "test_env::",
    "tui::signals::tests::a_thread_started_unsignalled_is_in_the_process_of_the_case_that_started_it",
    "tui::worker::tests::a_launched_worker_reads_the_whole_fake_process_it_was_launched_in",
    "permission::authority::tests::a_command_plan_never_inherits_the_process_environment",
]
CHECK = ["test_env::tests::every_read_outside_process_env_is_bound_to_its_variables"]

TERM = "src/tui/term.rs"
TMUX_READ = 'std::env::var_os("TMUX").is_some_and'
TMUX_REASON = 'reason = "env: TMUX -- whether the terminal is tmux\'s"'

# (kind, name, [(file, old, new), ...])
ENTRIES = [
    # Round 1's mutants, at round 2's text.
    ("test", "guard-gone: process_env! var/var_os read std::env in tests", [(
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
    };""")]),
    ("test", "refusal-gone: test_env::var_os returns None outside a fake", [(
        "src/test_env.rs",
        "        None => refuse(who, name),\n    }\n}\n\n/// `std::env::var(name)`, as `who`",
        "        None => { let _ = who; None }\n    }\n}\n\n/// `std::env::var(name)`, as `who`")]),
    ("test", "config-vars-unrouted: Environment::from_process reads its variables from std::env", [(
        "src/config.rs",
        'process_env!("config::Environment::from_process", var(key))',
        "std::env::var(key)")]),
    ("test", "config-home-unrouted: Environment::from_process reads HOME from std::env", [(
        "src/config.rs",
        'process_env!("config::Environment::from_process", var_os("HOME"))',
        'std::env::var_os("HOME")')]),
    ("test", "gateway-reader-unrouted: Endpoint::from_process reads std::env", [(
        "src/gateway/mod.rs",
        'process_env!("gateway::Endpoint::from_process", var(GATEWAY_URL_ENV))',
        "std::env::var(GATEWAY_URL_ENV)")]),
    ("test", "git-reader-unrouted: GitEnvironment::from_process reads std::env", [(
        "src/permission/gitconfig.rs",
        'process_env!("permission::GitEnvironment::from_process", var_os(name))',
        "std::env::var_os(name)")]),
    ("test", "home-to-git-unrouted: minimal_environment's HOME is the process's", [(
        "src/permission/authority.rs",
        'process_env!(\n        "permission::authority::minimal_environment",\n        child_var("HOME")\n    )',
        'std::env::var("HOME")')]),
    ("test", "child-isolation-gone: process::command inherits the process", [(
        "src/process.rs",
        "    #[cfg(test)]\n    crate::test_env::isolate_child(&mut command);\n",
        "")]),
    ("test", "carry-gone: spawn_unsignalled does not carry the fake process", [(
        "src/tui/signals.rs",
        "    #[cfg(test)]\n    let body = crate::test_env::carried(body);\n",
        "")]),
    ("test", "mcp-fallback-back: with_environment falls back to std::env", [(
        "src/mcp/runtime.rs",
        "Box::new(move |name| overrides.get(name).cloned().or_else(|| process(name))),",
        "Box::new(move |name| { let _ = &process; overrides.get(name).cloned().or_else(|| std::env::var(name).ok()) }),")]),
    # Round 2's mutants.
    ("test", "overlay-gone: a runtime's environment replaces the thread's fake (item 3)", [(
        "src/test_env.rs",
        "                Some(under) => {\n                    let mut vars = under.vars.clone();\n                    vars.extend(self.vars);\n                    FakeProcess { vars }\n                }",
        "                Some(_) => self,")]),
    ("test", "global-git-gone: a homeless plan keeps the global configuration (item 2)", [(
        "src/test_env.rs",
        "    if homeless\n",
        "    if false && homeless\n")]),
    ("test", "proxy-rule-gone: bearer_rules takes the process's proxy in a test (item 6)", [(
        "src/gateway/mod.rs",
        "    #[cfg(test)]\n    let builder = crate::test_env::proxies(builder);\n",
        "")]),
    ("test", "fake-proxy-ignored: proxies() drops the fake process's proxy (item 6)", [(
        "src/test_env.rs",
        "            builder = builder.proxy(proxy.no_proxy(exempt.clone()));",
        "            let _ = (proxy, &exempt);")]),
    # Item 1's fixtures.
    ("clippy", "fixture: an imported var", [(
        TERM, "pub(crate) fn under_tmux() -> bool {",
        "pub(crate) fn fixture_imported() -> bool {\n    use std::env::var;\n    var(\"HOME\").is_ok()\n}\n\npub(crate) fn under_tmux() -> bool {")]),
    ("clippy", "fixture: an aliased std::env", [(
        TERM, "pub(crate) fn under_tmux() -> bool {",
        "pub(crate) fn fixture_aliased() -> bool {\n    use std::env as process;\n    process::var_os(\"HOME\").is_some()\n}\n\npub(crate) fn under_tmux() -> bool {")]),
    ("clippy", "fixture: a reader passed as a function", [(
        TERM, "pub(crate) fn under_tmux() -> bool {",
        "pub(crate) fn fixture_pointer() -> bool {\n    Some(\"HOME\").and_then(std::env::var_os).is_some()\n}\n\npub(crate) fn under_tmux() -> bool {")]),
    ("check", "fixture: a key swap at an allowed site", [(
        TERM, TMUX_READ, 'std::env::var_os("HOME").is_some_and')]),
    ("check", "fixture: a key swap with its reason", [
        (TERM, TMUX_READ, 'std::env::var_os("HOME").is_some_and'),
        (TERM, TMUX_REASON, 'reason = "env: HOME -- whether the terminal is tmux\'s"')]),
    ("check", "fixture: a key swap with its reason and its listing", [
        (TERM, TMUX_READ, 'std::env::var_os("HOME").is_some_and'),
        (TERM, TMUX_REASON, 'reason = "env: HOME -- whether the terminal is tmux\'s"'),
        ("src/test_env/tests.rs", '("src/tui/term.rs", &["TMUX"]),', '("src/tui/term.rs", &["HOME"]),')]),
    ("check", "fixture: an env: allow over a start", [(
        TERM, TMUX_READ, 'drop(std::process::Command::new("true"));\n    std::env::var_os("TMUX").is_some_and')]),
]


def run(cmd, cwd):
    return subprocess.run(cmd, cwd=cwd, text=True, capture_output=True)


def lib_tests(clone):
    out = run(["cargo", "test", "--locked", "--lib", "--no-run", "--message-format=json"], clone)
    for line in out.stdout.splitlines():
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        if (msg.get("reason") == "compiler-artifact" and msg.get("executable")
                and msg["target"]["kind"] == ["lib"]):
            return msg["executable"], ""
    return None, out.stderr[-1500:]


def main():
    clone = pathlib.Path(sys.argv[1]).resolve()
    head = sys.argv[2]
    assert (clone / ".git").is_dir(), "a clone, not a worktree"
    assert run(["git", "checkout", "-q", "--force", head], clone).returncode == 0
    for _, name, edits in ENTRIES:
        for path, old, _ in edits:
            assert (clone / path).read_text().count(old) == 1, f"{name}: does not apply"
    survived = leaked = 0
    for kind, name, edits in ENTRIES:
        assert run(["git", "checkout", "-q", "--force", head], clone).returncode == 0
        for path, old, new in edits:
            target = clone / path
            target.write_text(target.read_text().replace(old, new))
        if kind == "clippy":
            out = run(["cargo", "clippy", "--locked", "--all-targets", "--", "-D", "warnings"], clone)
            killed = out.returncode != 0 and "use of a disallowed method" in out.stderr
            hits = sorted({line.strip() for line in out.stderr.splitlines()
                           if "use of a disallowed method" in line})
            verdict = "KILLED" if killed else "SURVIVED"
            print(f"{verdict} {name} :: clippy exit {out.returncode}; {'; '.join(hits)}")
            survived += not killed
            continue
        binary, why = lib_tests(clone)
        if binary is None:
            print(f"KILLED (does not build) {name}")
            continue
        result = run([str(SENTINEL_RUN), binary, *(CHECK if kind == "check" else CASES)], clone)
        output = result.stdout + result.stderr
        killed = "test result: FAILED" in result.stdout and "running 0 tests" not in result.stdout
        found = [value for value in SENTINEL_VALUES if value in output]
        summary = [line for line in result.stdout.splitlines() if line.startswith("test result")]
        verdict = "KILLED" if killed else "SURVIVED"
        survived += not killed
        leaked += bool(found)
        print(f"{verdict} {name} :: {summary[-1] if summary else 'no result'} :: "
              f"sentinel values in output: {len(found)}")
    run(["git", "checkout", "-q", "--force", head], clone)
    print(f"total {len(ENTRIES)}, survived {survived}, runs with a sentinel value in output {leaked}")
    sys.exit(1 if survived or leaked else 0)


if __name__ == "__main__":
    main()
