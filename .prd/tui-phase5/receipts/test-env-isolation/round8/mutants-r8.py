#!/usr/bin/env python3
"""Round 8's mutants: round 7's, at round 8's text, and three for the rule
round 8 adds (cfg_select!), on a throwaway clone only. Its first output line
names the revision.

    TMPDIR=<short> mutants-r8.py <clone> <head> [name filter...]

<clone> is a `git clone --no-local` copy, never the worktree. For each entry:
reset the clone to <head>, apply its edits (an `old` of None writes a new
file), then

- "test": build the lib tests and run CASES under sentinel-run.sh; KILLED
  when a case fails;
- "envcheck": run scripts/check-env-reads.py's own `check_tree`, its clippy
  passes and test builds narrowed to what the hole needs (the dev profile,
  default and all features, and no test build, unless the entry names
  more); KILLED when it refuses. Each narrowing is first run on the
  unmutated head, which must pass, so a kill is the mutant's;
- "selftest": run scripts/check-env-reads.py --self-test; KILLED when a
  fixture is not refused for its reason;
- "isolation": build the integration tests and run tests/isolation.rs under
  sentinel-run.sh; KILLED when the suite does not pass.

Every test run is also searched for sentinel-run.sh's values: a failing case
must say which variable differs, never what it holds. The values are
counted, not printed. Exits non-zero if anything survives, a baseline
fails, or a value leaks.
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

TERM = "src/tui/term.rs"
TMUX_READ = 'env_exception!(var_os("TMUX")).is_some_and'
ISOLATED = "tests/support/isolated.rs"
UNIT = "src/test_env/tests.rs"
UNIT_ANCHOR = "/// A comparison of a value read from a (fake) process environment: on a"

DEV = '([], ["--all-features"])'
RELEASE = '([], ["--all-features"], ["--release"])'
NO_BUILDS = "()"
DEV_BUILD = "([],)"

# (kind, name, [(file, old, new), ...], passes, test builds)
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
    # Round 4's start-scan fixtures, now held by clippy's Command::new.
    ("envcheck", "bare-command-in-a-test-module (round 2 SHOULD 2)", [(
        "src/tui/file_index.rs", "            let mut command = crate::process::command(&git);",
        "            let mut command = std::process::Command::new(&git);")]),
    ("envcheck", "probe-bare: the SIGCHLD probe made with a bare Command (round 4, item 3)", [(
        "src/process.rs",
        '        let mut probe = command(std::env::current_exe().expect("this binary"));',
        '        let mut probe = std::process::Command::new(std::env::current_exe().expect("this binary"));')]),
    # Round 5, part 1.
    ("envcheck", "aliased-git: opus M1's case, an aliased Command in a src test module", [(
        UNIT, UNIT_ANCHOR,
        '#[allow(dead_code, clippy::disallowed_methods)]\nfn a_git_with_the_real_home() {\n    use std::process::Command as Git;\n'
        '    let _ = crate::process::output(&mut Git::new("git").args(["config", "--global", "--list"]));\n}\n\n' + UNIT_ANCHOR)]),
    ("envcheck", "tests-start-outside-support: a Command::new in a test file", [(
        "tests/parity.rs", '    let mut command = support::command(env!("CARGO_BIN_EXE_xfx"));',
        '    #[allow(clippy::disallowed_methods)]\n    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_xfx"));')]),
    ("envcheck", "config-loses-command-new: clippy.toml no longer disallows Command::new", [(
        "clippy.toml",
        '  { path = "std::process::Command::new", reason = "a child is made by crate::process::command, which gives a unit test\'s child the case\'s fake process, never the developer\'s" },\n',
        "")]),
    ("envcheck", "dot-clippy-toml: a .clippy.toml beside clippy.toml", [(".clippy.toml", None, "")]),
    # Round 5, part 2.
    ("envcheck", "option-env-key: option_env!(XFX_GATEWAY_KEY) in a src test", [(
        UNIT, UNIT_ANCHOR,
        '#[test]\nfn reads_a_key_at_compile_time() {\n    assert!(option_env!("XFX_GATEWAY_KEY").is_none() || true);\n}\n\n' + UNIT_ANCHOR)]),
    ("envcheck", "env-in-concat: an unlisted env! nested in concat!", [(
        UNIT, UNIT_ANCHOR,
        '#[allow(dead_code)]\nconst BY: &str = concat!("by ", env!("CARGO_PKG_AUTHORS"));\n\n' + UNIT_ANCHOR)]),
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
        "tests/escape.rs", None, "#[test]\nfn runs_in_the_developers_process() {}\n")]),
    # Round 5, part 3: seen only by the release pass.
    ("envcheck", "release-only-read: a read under cfg(not(debug_assertions))", [(
        UNIT, UNIT_ANCHOR,
        '#[cfg(not(debug_assertions))]\n#[test]\n#[allow(clippy::disallowed_methods)]\nfn a_release_only_read() {\n'
        '    let _ = std::env::var_os("HOME");\n}\n\n' + UNIT_ANCHOR)], RELEASE, NO_BUILDS),
    # Round 5, part 4: seen only in the built test binaries.
    ("envcheck", "cfg-clippy-support: a test file whose support only clippy compiles", [(
        "tests/escape.rs", None,
        "#[cfg(clippy)]\nmod support;\n\n#[test]\nfn runs_in_the_developers_process() {}\n")], DEV, DEV_BUILD),
    ("envcheck", "bin-harness-with-a-test: a #[test] in src/main.rs", [(
        "src/main.rs", None, "\n#[cfg(test)]\nmod t {\n    #[test]\n    fn runs() {}\n}\n")], DEV, DEV_BUILD),
    ("selftest", "macro-takes-expr: env_exception!'s var_os arm takes any expression", [(
        "src/env_exception.rs", "    (var_os($name:literal)) => {", "    (var_os($name:expr)) => {")]),
    ("selftest", "self-test-dev-only: the self-test compiles its fixtures in the dev profile only", [(
        "scripts/check-env-reads.py", '        for build in ([], ["--release"]):', "        for build in ([],):")]),
    ("selftest", "no-dep-info: rustc's env-dep names are not read", [(
        "scripts/check-env-reads.py", '        if line.startswith("# env-dep:"):', "        if False:")]),
    ("selftest", "probe-gone: a test binary is never started", [(
        "scripts/check-env-reads.py", '            if probe.returncode != 101 or b"no case runs" not in probe.stderr:', "            if False:")]),
    ("selftest", "no-macro-lint: disallowed_macros is not forced", [(
        "scripts/check-env-reads.py", ', "--force-warn", "clippy::disallowed_macros"]', "]")]),
    # Round 4, part 2: whole-binary isolation.
    ("isolation", "marker-trusted: the copy runs its cases on the marker alone", [(
        ISOLATED, "        Some(marker) => confirm(&marker).err(),", "        Some(_) => None,")]),
    ("isolation", "home-not-empty: the copy takes a HOME with something in it", [(
        ISOLATED, "    if entries.next().is_some() {", "    if false && entries.next().is_some() {")]),
    ("isolation", "names-unchecked: the copy takes a name it was not given", [(
        ISOLATED, "    if !unexpected.is_empty() {", "    if false && !unexpected.is_empty() {")]),
    ("isolation", "no-constructor: nothing runs before main", [(
        ISOLATED,
        '#[used]\n#[cfg_attr(target_os = "linux", link_section = ".init_array")]\n#[cfg_attr(target_vendor = "apple", link_section = "__DATA,__mod_init_func")]\nstatic ISOLATE: extern "C" fn() = isolate;\n',
        "")]),
    ("isolation", "inherits: the copy is handed the developer's whole environment", [(
        ISOLATED, "    super::sandbox::clear_env(&mut copy);\n", "")]),
    ("isolation", "trailing-slash-gone: the copy takes a HOME ending in /", [(
        ISOLATED, '    if home.as_os_str().as_bytes().ends_with(b"/") {', "    if false {")]),
    ("isolation", "exact-path-gone: the copy compares HOME's last component only (round 4)", [(
        ISOLATED, "    if home.as_os_str() != home_for(marker)?.as_os_str() {",
        "    if home.file_name() != home_for(marker)?.file_name() {")]),
    ("isolation", "value-in-error: the creation error prints the path", [(
        ISOLATED, '        return format!("cannot make the copy\'s home under TMPDIR: {}", err.kind());',
        '        return format!("cannot make the copy\'s home {}: {}", home.display(), err.kind());')]),
    # Round 6, MUST 1: a cfg naming clippy.
    ("envcheck", "cfg-not-clippy: a HOME read in a unit test under cfg(all(test, not(clippy)))", [(
        UNIT, UNIT_ANCHOR,
        "#[cfg(all(test, not(clippy)))]\n#[test]\nfn read_where_clippy_never_looks() {\n"
        '    let _ = std::env::var_os("HOME");\n}\n\n' + UNIT_ANCHOR)]),
    ("envcheck", "cfg-attr-clippy: a cfg_attr whose predicate names clippy", [(
        UNIT, UNIT_ANCHOR, "#[cfg_attr(not(clippy), allow(dead_code))]\nfn f() {}\n\n" + UNIT_ANCHOR)]),
    ("selftest", "cfg-check-gone: a cfg naming clippy is not refused", [(
        "scripts/check-env-reads.py",
        "            predicate = text[call.end() : predicate_end if predicate_end is not None else at - 1]\n            if CLIPPY_WORD.search(predicate):",
        "            predicate = text[call.end() : predicate_end if predicate_end is not None else at - 1]\n            if False:")]),
    # Round 6, MUST 2: the exec family and the raw system call.
    ("envcheck", "syscall-plant: a raw syscall in a unit test", [(
        UNIT, UNIT_ANCHOR,
        "#[test]\nfn a_raw_system_call() {\n    // SAFETY: getpid takes nothing.\n"
        "    let _ = unsafe { libc::syscall(20) };\n}\n\n" + UNIT_ANCHOR)]),
    ("envcheck", "config-loses-syscall: clippy.toml no longer disallows libc::syscall", [(
        "clippy.toml",
        '  { path = "libc::syscall", reason = "a raw system call can start, replace or fork the process past every rule here" },\n',
        "")]),
    # Round 6, MUST 3: dep-info.
    ("selftest", "missing-dep-info-passes: a missing .d is an empty set again", [(
        "scripts/check-env-reads.py", "    return (names, sources) if found else None", "    return (names, sources)")]),
    ("selftest", "lib-strip-everywhere: lib is stripped from an executable's name too", [(
        "scripts/check-env-reads.py", '    if hashed and (hashed.group("ext") or not hashed.group("lib")):', "    if hashed:")]),
    # Round 6, SHOULDs.
    ("envcheck", "user-database: getpwuid in a unit test", [(
        UNIT, UNIT_ANCHOR,
        "#[test]\nfn the_home_from_the_user_database() {\n    // SAFETY: getuid cannot fail.\n"
        "    let _ = unsafe { libc::getpwuid(libc::getuid()) };\n}\n\n" + UNIT_ANCHOR)]),
    ("envcheck", "second-feature: a feature the matrix does not cover the subsets of", [(
        "Cargo.toml", "fault-injection = []\n", "fault-injection = []\nanother = []\n")]),
    ("envcheck", "release-all-features-escape: support compiled in every build but release with all features", [(
        "tests/escape.rs", None,
        '#[cfg(any(debug_assertions, not(feature = "fault-injection")))]\nmod support;\n\n'
        "#[test]\nfn runs_in_the_developers_process() {}\n")], DEV, '(["--release", "--all-features"],)'),
    ("isolation", "relative-tmpdir: the copy takes a TMPDIR that is not absolute", [(
        ISOLATED, "    if !temporary.is_absolute() {", "    if false {")]),
    # Round 7, MUST 1: the cfg check reads code, every compiled file.
    ("envcheck", "cfg-behind-a-comment-paren: #[cfg(/*)*/ all(test, not(clippy)))] in a unit test", [(
        UNIT, UNIT_ANCHOR,
        "#[cfg(/*)*/ all(test, not(clippy)))]\n#[test]\nfn hidden_behind_a_comment() {\n"
        '    let _ = std::env::var_os("HOME");\n}\n\n' + UNIT_ANCHOR)]),
    ("envcheck", "cfg-in-a-path-module: a #[path] module file that is not .rs", [
        ("src/test_env/tests.rs", UNIT_ANCHOR, '#[path = "hidden.inc"]\nmod hidden;\n\n' + UNIT_ANCHOR),
        ("src/test_env/hidden.inc", None, "#[cfg(not(clippy))]\n#[test]\nfn f() {}\n")]),
    ("selftest", "lexer-gone: comments and strings read as code again", [(
        "scripts/check-env-reads.py", "            text = code_of(source.read())", "            text = source.read()")]),
    ("selftest", "compiled-files-unread: the dep-info source lists are not read", [(
        "scripts/check-env-reads.py", "        files |= inside\n", "")]),
    # Round 7, MUST 2: a .d must hold the artifact's own rule.
    ("selftest", "any-d-is-evidence: an empty or cut .d counts again", [(
        "scripts/check-env-reads.py", "        if target in targets and paths:", "        if True:")]),
    ("selftest", "unrecorded-gone: a read clippy saw is not looked for in dep-info", [(
        "scripts/check-env-reads.py",
        "        for name in sorted(names - compiled.get(source, set())):",
        "        for name in []:")]),
    # Round 7, SHOULDs.
    ("envcheck", "rustc-cfg: build.rs sets a cfg", [(
        "build.rs", '    println!("cargo:rerun-if-changed=build.rs");',
        '    println!("cargo:rerun-if-changed=build.rs");\n    println!("cargo:rustc-cfg=xfx_hidden");')]),
    ("envcheck", "default-feature: a default feature in Cargo.toml", [(
        "Cargo.toml", "fault-injection = []\n", "default = []\nfault-injection = []\n")]),
    ("envcheck", "user-database-walk: getpwent in a unit test", [(
        UNIT, UNIT_ANCHOR,
        "#[test]\nfn the_homes_of_every_user() {\n    // SAFETY: getpwent takes nothing.\n"
        "    let _ = unsafe { libc::getpwent() };\n}\n\n" + UNIT_ANCHOR)]),
    ("selftest", "uplift-first-match: an uplifted file traced to the first byte-identical candidate", [(
        "scripts/check-env-reads.py",
        "    return dep_info(same_file[0]) if len(same_file) == 1 else None",
        "    return dep_info(same_file[0]) if same_file else None")]),
    # Round 8: cfg_select!.
    ("selftest", "cfg-select-dropped: cfg_select! is not matched", [(
        "scripts/check-env-reads.py", '    for found in CFG_SELECT.finditer(text):', "    for found in []:")]),
    ("selftest", "cfg-select-braces-only: only cfg_select! { .. } is matched", [(
        "scripts/check-env-reads.py", 'CFG_SELECT = re.compile(r"\\bcfg_select\\s*!\\s*[({\\[]")',
        'CFG_SELECT = re.compile(r"\\bcfg_select\\s*!\\s*[{]")')]),
    ("selftest", "cfg-select-scans-bodies: the arms' bodies are read as predicates too", [(
        "scripts/check-env-reads.py", "            yield found.start(), text[at:arrow]",
        "            yield found.start(), text[found.end():body_end]")]),
    ("envcheck", "cfg-select-plant: opus's cfg_select! in a unit-test module", [(
        UNIT, UNIT_ANCHOR,
        "cfg_select! {\n    clippy => {}\n    _ => {\n        #[test]\n        fn where_clippy_never_looks() {\n"
        '            let _ = std::env::var_os("HOME");\n        }\n    }\n}\n\n' + UNIT_ANCHOR)]),
]


HARNESS = (
    "import importlib.util, sys\n"
    "sys.dont_write_bytecode = True\n"
    "spec = importlib.util.spec_from_file_location('check', 'scripts/check-env-reads.py')\n"
    "check = importlib.util.module_from_spec(spec)\n"
    "spec.loader.exec_module(check)\n"
    "check.PASSES = eval(sys.argv[1])\n"
    "check.TEST_BUILDS = eval(sys.argv[2])\n"
    "sys.exit(check.check_tree())\n"
)


def run(cmd, cwd):
    return subprocess.run(cmd, cwd=cwd, text=True, capture_output=True)


def apply(clone, edits):
    for path, old, new in edits:
        target = clone / path
        if old is None:
            before = target.read_text() if target.exists() else ""
            target.write_text(before + new)
        else:
            target.write_text(target.read_text().replace(old, new))


def envcheck(clone, passes, builds):
    out = run(["python3", "-I", "-c", HARNESS, passes, builds], clone)
    why = [line.strip() for line in out.stdout.splitlines() if line.strip().startswith("refused:")]
    if out.returncode != 0 and not why:
        why = [line.strip() for line in out.stdout.splitlines() if line.startswith("check-env-reads:")][-1:]
        why += [line.strip() for line in out.stderr.splitlines() if line.startswith(("error", "Traceback"))][:2]
    return out.returncode, why, out.stdout


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
    only = sys.argv[3:]
    assert (clone / ".git").is_dir(), "a clone, not a worktree"
    assert run(["git", "checkout", "-q", "--force", head], clone).returncode == 0
    entries = [entry if len(entry) == 5 else (*entry, DEV, NO_BUILDS) for entry in ENTRIES]
    for _, name, edits, _, _ in entries:
        for path, old, _ in edits:
            if old is not None:
                assert (clone / path).read_text().count(old) == 1, f"{name}: does not apply"
    print(f"# mutants at {head}")
    survived = leaked = failed_baselines = 0
    chosen = [entry for entry in entries if not only or any(word in entry[1] for word in only)]
    baselines = sorted({(passes, builds) for kind, _, _, passes, builds in chosen if kind == "envcheck"})
    for passes, builds in baselines:
        run(["git", "checkout", "-q", "--force", head], clone)
        status, why, _ = envcheck(clone, passes, builds)
        failed_baselines += status != 0
        print(f"BASELINE {'ok' if status == 0 else 'FAILED'} :: envcheck passes {passes}, test builds {builds}; " + " | ".join(why[:4]))
    for kind, name, edits, passes, builds in chosen:
        assert run(["git", "checkout", "-q", "--force", head], clone).returncode == 0
        run(["git", "clean", "-q", "-fd", "--", "tests", "src"], clone)
        for path in (".clippy.toml",):
            (clone / path).unlink(missing_ok=True)
        apply(clone, edits)
        if kind == "envcheck":
            status, why, _ = envcheck(clone, passes, builds)
            killed = status != 0
            verdict = "KILLED" if killed else "SURVIVED"
            print(f"{verdict} {name} :: exit {status}; " + " | ".join(why[:4]))
            survived += not killed
            continue
        if kind == "selftest":
            out = run(["python3", "-I", "scripts/check-env-reads.py", "--self-test"], clone)
            killed = out.returncode != 0
            why = [line.strip() for line in out.stdout.splitlines()
                   if line.strip().startswith(("NOT REFUSED", "REFUSED", "NOT RESOLVED"))]
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
            summary = [line for line in result.stdout.splitlines() if line.startswith("test result")]
            # The suite's own last line: a failing case's message can hold a
            # copy's "test result: ok." of its own.
            killed = not summary or not summary[-1].startswith("test result: ok.")
            found = [value for value in SENTINEL_VALUES if value in output]
            verdict = "KILLED" if killed else "SURVIVED"
            survived += not killed
            leaked += bool(found)
            # With no result line, what refused: the copy's own refusal, its
            # list of names cut, never a value.
            refusal = next((line.split("no case runs: ", 1)[1].split(": [")[0]
                            for line in result.stderr.splitlines() if "no case runs: " in line), None)
            shown = summary[-1] if summary else (f"no result; refused: {refusal}" if refusal else "no result")
            print(f"{verdict} {name} :: {shown} :: sentinel values in output: {len(found)}")
            continue
        binary, why = lib_tests(clone)
        if binary is None:
            print(f"KILLED (does not build) {name}")
            continue
        result = run([str(SENTINEL_RUN), binary, *CASES], clone)
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
    run(["git", "clean", "-q", "-fd", "--", "tests", "src"], clone)
    (clone / ".clippy.toml").unlink(missing_ok=True)
    print(f"total {len(chosen)}, survived {survived}, failed baselines {failed_baselines}, "
          f"runs with a sentinel value in output {leaked}")
    sys.exit(1 if survived or leaked or failed_baselines else 0)


if __name__ == "__main__":
    main()
