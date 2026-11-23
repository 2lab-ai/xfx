#!/usr/bin/env python3
"""Round 4's mutants (round 1-3's that still apply at round 4's text, and
the new ones), on a throwaway clone only.

    TMPDIR=<short> mutants-r4.py <clone> <head> [name filter]

<clone> is a `git clone --no-local` copy, never the worktree. For each entry:
reset the clone to <head>, apply one textual edit, then

- "test": build the lib tests and run CASES under sentinel-run.sh; KILLED
  when a case fails;
- "startscan": the same, running the start rule's line check; KILLED when
  it fails;
- "envcheck": run scripts/check-env-reads.py; KILLED when it refuses;
- "selftest": run scripts/check-env-reads.py --self-test; KILLED when a
  fixture is not refused for its reason;
- "isolation": build the integration tests and run tests/isolation.rs under
  sentinel-run.sh; KILLED when a case fails or the binary runs none.

Every test run is also searched for sentinel-run.sh's values: a failing case
must say which variable differs, never what it holds. The values are
counted, not printed. Exits non-zero if anything survives or leaks.
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
    "xfx-sentinel-git-token",
    "sentinel.invalid",
    "tei-sentinel",
    "xfx-sentinel-profile-model",
]
CASES = [
    "test_env::",
    "tui::signals::tests::a_thread_started_unsignalled_is_in_the_process_of_the_case_that_started_it",
    "tui::worker::tests::a_launched_worker_reads_the_whole_fake_process_it_was_launched_in",
    "permission::authority::tests::a_command_plan_never_inherits_the_process_environment",
]
STARTSCAN = ["process::tests::no_child_process_is_made_any_other_way"]

TERM = "src/tui/term.rs"
TMUX_READ = 'env_exception!(var_os("TMUX")).is_some_and'
ISOLATED = "tests/support/isolated.rs"

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
    # Round 2's bare-Command fixture, now that process.rs's test module is read.
    ("startscan", "fixture: a bare Command in a test module (round 2 SHOULD 2)", [(
        "src/tui/file_index.rs", "            let mut command = crate::process::command(&git);",
        "            let mut command = std::process::Command::new(&git);")]),
    ("startscan", "probe-bare: the SIGCHLD probe made with a bare Command (round 4, item 3)", [(
        "src/process.rs",
        '        let mut probe = command(std::env::current_exe().expect("this binary"));',
        '        let mut probe = std::process::Command::new(std::env::current_exe().expect("this binary"));')]),
    # Round 4, part 1: what clippy is told to let through.
    ("envcheck", "allowed-raw-read: an allow over a plain read, outside the macro", [(
        TERM, TMUX_READ,
        '{\n        #[allow(clippy::disallowed_methods)]\n        let tmux = std::env::var_os("TMUX");\n        tmux\n    }.is_some_and')]),
    ("envcheck", "crate-allow: allow(clippy::all, warnings) at the crate level, then a plain read", [
        ("src/lib.rs", "/// `env_exception!`, the literal-named reads", "#![allow(clippy::all, warnings)]\n\n/// `env_exception!`, the literal-named reads"),
        (TERM, TMUX_READ, 'std::env::var_os("TMUX").is_some_and')]),
    ("envcheck", "expect-and-cfg-attr: a read under expect, one under cfg_attr(allow)", [
        (TERM, "pub(crate) fn under_tmux() -> bool {",
         '#[expect(clippy::disallowed_methods)]\n#[cfg_attr(all(), allow(clippy::disallowed_methods))]\npub(crate) fn under_tmux() -> bool {'),
        (TERM, TMUX_READ, 'std::env::var_os("TMUX").is_some_and')]),
    ("envcheck", "name-swap: a listed invocation reads HOME", [(
        TERM, TMUX_READ, 'env_exception!(var_os("HOME")).is_some_and')]),
    ("envcheck", "unlisted: an invocation the list does not hold", [(
        "scripts/check-env-reads.py", '    ("src/tui/term.rs", ("TMUX",)),\n', "")]),
    ("envcheck", "gone: a listed invocation no longer there", [(
        "src/permission/gitconfig.rs", '        .or_else(|| env_exception!(var_os("PATH")))', "        .or_else(|| None)")]),
    ("envcheck", "start: an allow over a start outside process::spawn", [(
        "src/process.rs",
        "pub fn status(command: &mut Command) -> io::Result<ExitStatus> {\n    spawn(command)?.wait()\n}",
        "pub fn status(command: &mut Command) -> io::Result<ExitStatus> {\n    #[allow(clippy::disallowed_methods)]\n    let status = command.status();\n    status\n}")]),
    ("envcheck", "build-script: one read more in build.rs", [(
        "build.rs", '    println!("cargo:rerun-if-changed=build.rs");',
        '    println!("cargo:rerun-if-changed=build.rs");\n    let _home = std::env::var("HOME");')]),
    ("envcheck", "escape: a test binary that does not declare mod support", [(
        "tests/parity.rs", "// Runs this binary's cases in a copy of it whose environment is built from\n// nothing (`support::isolated`).\nmod support;\n", "")]),
    ("selftest", "macro-takes-expr: env_exception!'s var_os arm takes any expression", [(
        "src/env_exception.rs", "    (var_os($name:literal)) => {", "    (var_os($name:expr)) => {")]),
    # Round 4, part 2: whole-binary isolation.
    ("isolation", "marker-trusted: the copy runs its cases on the marker alone", [(
        ISOLATED, "        Some(marker) => confirm(&marker).err(),", "        Some(_) => None,")]),
    ("isolation", "home-not-empty: the copy takes a HOME with something in it", [(
        ISOLATED, "    if entries.next().is_some() {", "    if false && entries.next().is_some() {")]),
    ("isolation", "home-not-named: the copy takes a HOME the marker does not name", [(
        ISOLATED, "    if home.file_name() != Some(", "    if false && home.file_name() != Some(")]),
    ("isolation", "names-unchecked: the copy takes a name it was not given", [(
        ISOLATED, "    if !unexpected.is_empty() {", "    if false && !unexpected.is_empty() {")]),
    ("isolation", "no-constructor: nothing runs before main", [(
        ISOLATED,
        '#[used]\n#[cfg_attr(target_os = "linux", link_section = ".init_array")]\n#[cfg_attr(target_vendor = "apple", link_section = "__DATA,__mod_init_func")]\nstatic ISOLATE: extern "C" fn() = isolate;\n',
        "")]),
    ("isolation", "inherits: the copy is handed the developer's whole environment", [(
        ISOLATED, "    super::sandbox::clear_env(&mut copy);\n", "")]),
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
    only = sys.argv[3] if len(sys.argv) > 3 else None
    for kind, name, edits in ENTRIES:
        if only and only not in name:
            continue
        assert run(["git", "checkout", "-q", "--force", head], clone).returncode == 0
        for path, old, new in edits:
            target = clone / path
            target.write_text(target.read_text().replace(old, new))
        if kind in ("envcheck", "selftest"):
            out = run(["python3", "-I", "scripts/check-env-reads.py"] + (["--self-test"] if kind == "selftest" else []), clone)
            killed = out.returncode != 0
            why = [line.strip() for line in out.stdout.splitlines()
                   if line.strip().startswith(("refused:", "NOT REFUSED", "REFUSED"))
                   or "clippy" in line and "exited" in line]
            verdict = "KILLED" if killed else "SURVIVED"
            print(f"{verdict} {name} :: exit {out.returncode}; " + " | ".join(why[:4]))
            survived += not killed
            continue
        if kind == "isolation":
            out = run(["cargo", "test", "--locked", "--tests", "--no-run"], clone)
            binary = next((clone / line.split("(")[1].rstrip(")") for line in out.stderr.splitlines()
                           if "Executable tests/isolation.rs" in line), None)
            if binary is None:
                print(f"KILLED (does not build) {name}")
                continue
            result = run([str(SENTINEL_RUN), str(binary), "--test-threads=4"], clone)
            output = result.stdout + result.stderr
            killed = "test result: ok." not in result.stdout
            found = [value for value in SENTINEL_VALUES if value in output]
            summary = [line for line in result.stdout.splitlines() if line.startswith("test result")]
            verdict = "KILLED" if killed else "SURVIVED"
            survived += not killed
            leaked += bool(found)
            print(f"{verdict} {name} :: {summary[-1] if summary else 'no result'} :: "
                  f"sentinel values in output: {len(found)}")
            continue
        binary, why = lib_tests(clone)
        if binary is None:
            print(f"KILLED (does not build) {name}")
            continue
        result = run([str(SENTINEL_RUN), binary, *(STARTSCAN if kind == "startscan" else CASES)], clone)
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
    ran = sum(1 for _, name, _ in ENTRIES if not only or only in name)
    print(f"total {ran}, survived {survived}, runs with a sentinel value in output {leaked}")
    sys.exit(1 if survived or leaked else 0)


if __name__ == "__main__":
    main()
