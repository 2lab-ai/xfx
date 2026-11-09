#!/usr/bin/env python3
"""Every read of the process environment, and every start of a child, that
clippy's `disallowed-methods` (clippy.toml) is told to let through.

    python3 scripts/check-env-reads.py              # the tree
    python3 scripts/check-env-reads.py --self-test  # the fixtures below

The check runs clippy itself, with the lint forced on
(`--force-warn clippy::disallowed_methods`): no `allow`, `expect`, group,
`warnings`, crate-level attribute, `cfg_attr` or `-A` can turn a forced lint
off, so every read and every start the lint knows of is reported, whatever
lets it through in the ordinary run. Clippy resolves each call's path, so an
import, an alias, a closure or a function reference is the call it names.
Nothing here reads source text to find a read: the reports are clippy's.

Each reported call is refused unless it is one of:

- an `env_exception!` arm (src/env_exception.rs): its invocation, read off the
  bytes of the compiler's own span of it, has exactly the literal form, its
  names are in ALLOWED_NAMES, and its file and names are an entry of
  EXCEPTIONS -- the list of every invocation, compared both ways, so an
  invocation added or one that goes missing fails alike;
- a `process_env!` build arm (src/lib.rs): names computed by design, read from
  the process in a build and from the case's fake process in a unit test
  (src/test_env.rs);
- a call outside any macro listed in PLAIN, by file, path and count: the
  start every start goes through, and the build script.

The values of the environment are never read here, and never printed.
"""

import collections
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The readers clippy.toml disallows for the environment; every other entry
# there is the start rule's.
ENV_READERS = {
    "std::env::var",
    "std::env::var_os",
    "std::env::vars",
    "std::env::vars_os",
    "std::env::home_dir",
    "libc::getenv",
}

# Every variable an `env_exception!` may name, and why it may: none names the
# person's home, profile or configuration, an xfx setting, a proxy or a
# credential. A list of what is allowed, not of what is not: a name nobody
# thought to forbid -- a token another tool reads -- is refused until someone
# adds it here with its reason.
ALLOWED_NAMES = {
    "PATH": "where programs are",
    "TMPDIR": "where temporary files go",
    "TEMP": "where temporary files go",
    "TMP": "where temporary files go",
    "TMUX": "whether the terminal is tmux's",
    "COLORFGBG": "the terminal's own colours",
    "COLORTERM": "the terminal's own colours",
    "TERM_PROGRAM": "the terminal's own name",
    "CARGO_PKG_NAME": "cargo's, in a test process",
    "XFX_TUI_FAULT": "a fault-injection point a test names",
    "XFX_TEST_USAGE_CACHE_KILL_BEFORE_RENAME": "a re-run test binary's role",
    "XFX_TEST_SIGCHLD_PROBE": "a re-run test binary's role",
    "XFX_TEST_PROXY_CASE_SERVER": "a re-run test binary's role",
    "XFX_TEST_IGNORED_AT_ENTRY": "a re-run test binary's role",
    "XFX_TEST_PROFILE_HOME_UNDER_OPEN_UMASK": "a re-run test binary's role",
    "XFX_TEST_UNDER_OPEN_UMASK": "a re-run test binary's role",
    "XFX_TEST_LOCAL_TIME_MOMENTS": "a re-run test binary's role",
    "XFX_TEST_USAGE_CACHE_ROLE": "a re-run test binary's role",
    "XFX_TEST_USAGE_CACHE_HOME": "a re-run test binary's role",
    "XFX_TEST_USAGE_CACHE_ARG": "a re-run test binary's role",
    "XFX_TEST_SENTINEL_RUNNER": "a re-run test binary's role",
    "XFX_TEST_ENV_PROXY_COPY": "a re-run test binary's role",
    "XFX_TEST_ISOLATED_COPY": "a re-run test binary's role",
    "XFX_DRAWN_WIDER_LIST": "the drawn-wider sweep's own",
    "XFX_DRAWN_WIDER_OUT": "the drawn-wider sweep's own",
}

# Every `env_exception!` invocation: its file and the names it reads, one
# entry per invocation.
EXCEPTIONS = [
    # Where programs are and where temporary files go.
    ("src/test_env.rs", ("PATH",)),
    ("src/test_env.rs", ("TMPDIR",)),
    ("src/permission/authority.rs", ("PATH",)),
    ("src/permission/gitconfig.rs", ("PATH",)),
    ("src/interactive.rs", ("TMPDIR",)),
    ("src/tui/shell.rs", ("TMPDIR",)),
    ("src/mcp/docker.rs", ("TMPDIR", "TEMP", "TMP")),
    ("tests/support/sandbox.rs", ("PATH",)),
    ("tests/support/sandbox.rs", ("TMPDIR",)),
    ("tests/cli.rs", ("PATH",)),
    ("tests/cli.rs", ("TMPDIR",)),
    # What the terminal says of itself.
    ("src/tui/term.rs", ("TMUX",)),
    ("src/tui/mod.rs", ("COLORFGBG",)),
    ("src/tui/mod.rs", ("COLORTERM",)),
    ("src/tui/mod.rs", ("TERM_PROGRAM",)),
    # A fault-injection point, and the roles a re-run test binary is given.
    ("src/tui/fault.rs", ("XFX_TUI_FAULT",)),
    ("src/usage/cache.rs", ("XFX_TEST_USAGE_CACHE_KILL_BEFORE_RENAME",)),
    ("src/process.rs", ("XFX_TEST_SIGCHLD_PROBE",)),
    ("src/test_net.rs", ("XFX_TEST_PROXY_CASE_SERVER",)),
    ("src/tui/signals.rs", ("XFX_TEST_IGNORED_AT_ENTRY",)),
    ("src/profile_home/tests.rs", ("XFX_TEST_PROFILE_HOME_UNDER_OPEN_UMASK",)),
    ("src/session/store/fork/tests.rs", ("XFX_TEST_UNDER_OPEN_UMASK",)),
    ("src/local_time/tests.rs", ("XFX_TEST_LOCAL_TIME_MOMENTS",)),
    ("src/usage/cache/tests.rs", ("XFX_TEST_USAGE_CACHE_ROLE",)),
    ("src/usage/cache/tests.rs", ("XFX_TEST_USAGE_CACHE_HOME",)),
    ("src/usage/cache/tests.rs", ("XFX_TEST_USAGE_CACHE_ARG",)),
    ("src/mcp/transport.rs", ("XFX_TEST_SENTINEL_RUNNER",)),
    ("src/mcp/transport.rs", ("XFX_TEST_SENTINEL_RUNNER",)),
    ("src/test_env/tests.rs", ("XFX_TEST_ENV_PROXY_COPY",)),
    ("tests/support/isolated.rs", ("XFX_TEST_ISOLATED_COPY",)),
    # An ignored case the drawn-wider sweep runs, with its own two.
    ("src/tui/drawn_wider.rs", ("XFX_DRAWN_WIDER_LIST",)),
    ("src/tui/drawn_wider.rs", ("XFX_DRAWN_WIDER_OUT",)),
    # Set by cargo in a test process: the case shows a command does not
    # inherit it.
    ("tests/permissions.rs", ("CARGO_PKG_NAME",)),
]

# Calls outside any macro that clippy is told to let through: file, path,
# how many.
PLAIN = [
    # The start every other start goes through (process::spawn and
    # process::spawn_unless), and the pipe every other pipe does
    # (process::pipe), under the lock.
    ("src/process.rs", "std::process::Command::spawn", 2),
    ("src/process.rs", "rustix::pipe::pipe", 1),
    # The build script: a process cargo runs before the crate exists, never a
    # test and never the product. It reads what cargo and a publishing
    # workflow set (XFX_BUILD_REVISION, XFX_BUILD_CHANNEL, PROFILE), drops
    # every GIT_* name from the git it starts, and starts that git (two
    # calls: `git_path` and `git_revision`).
    ("build.rs", "std::env::var", 3),
    ("build.rs", "std::env::vars_os", 1),
    ("build.rs", "std::process::Command::output", 2),
]

ENV_EXCEPTION_FILE = "src/env_exception.rs"
PROCESS_ENV_FILE = "src/lib.rs"

NAME = r'"([A-Z][A-Z0-9_]*)"'
SINGLE = re.compile(
    r"^(?:[a-z_][a-z0-9_]*::)?env_exception!\s*\(\s*(?:var|var_os)\s*\(\s*" + NAME + r"\s*\)\s*\)$"
)
LIST = re.compile(
    r"^(?:[a-z_][a-z0-9_]*::)?env_exception!\s*\(\s*first_set_os\s*\(\s*\[\s*"
    r'((?:"[A-Z][A-Z0-9_]*"\s*,\s*)*"[A-Z][A-Z0-9_]*")\s*,?\s*\]\s*\)\s*\)$'
)
DISALLOWED = re.compile(r"^use of a disallowed method `([^`]+)`")

# What the clippy run is not to take from the caller's environment: anything
# that changes which lints run or which configuration clippy reads.
DROPPED = (
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_RUSTFLAGS",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_BUILD_RUSTC_WRAPPER",
    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    "CLIPPY_CONF_DIR",
    "CLIPPY_DISABLE_DOCS_LINKS",
)


def clippy(crate, extra, env_extra=None):
    """Clippy's JSON messages for `crate`, the lint forced on."""
    env = {
        name: value
        for name, value in os.environ.items()
        if name not in DROPPED
        and not (name.startswith("CARGO_TARGET_") and name.endswith("_RUSTFLAGS"))
    }
    env.update(env_extra or {})
    command = (
        ["cargo", "clippy", "--all-targets", "--message-format=json"]
        + extra
        + ["--", "--force-warn", "clippy::disallowed_methods"]
    )
    run = subprocess.run(
        command, cwd=crate, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    messages = []
    finished = None
    for line in run.stdout.decode().splitlines():
        if not line.startswith("{"):
            continue
        record = json.loads(line)
        if record.get("reason") == "compiler-message":
            messages.append(record)
        elif record.get("reason") == "build-finished":
            finished = record.get("success")
    return run.returncode, finished, run.stderr.decode(), messages


def expansions(span):
    """The macro expansions a span came out of, innermost first."""
    chain = []
    expansion = span.get("expansion")
    while expansion:
        chain.append(expansion)
        expansion = expansion["span"].get("expansion")
    return chain


def invocation_names(crate, call):
    """The names an `env_exception!` invocation reads, from the bytes of the
    compiler's span of it, or None if it is not exactly the literal form."""
    with open(os.path.join(crate, call["file_name"]), "rb") as source:
        text = source.read()[call["byte_start"] : call["byte_end"]].decode()
    single = SINGLE.match(text)
    if single:
        return (single.group(1),)
    listed = LIST.match(text)
    if listed:
        return tuple(re.findall(NAME, listed.group(1)))
    return None


def classify(crate, messages, exception_file, process_env_file):
    """Sorts clippy's reports into env_exception! invocations,
    process_env! arms, plain calls and problems, each counted once whatever
    number of targets compiled it."""
    seen = set()
    invocations = []
    process_env = 0
    plain = collections.Counter()
    problems = []
    def_sites = collections.defaultdict(set)
    for record in messages:
        message = record["message"]
        primary = [span for span in message["spans"] if span["is_primary"]]
        where = primary[0] if primary else {"file_name": record["target"]["src_path"], "line_start": 0, "byte_start": 0}
        if message["level"] == "error":
            key = ("error", where["file_name"], where["byte_start"], message["message"])
            if key not in seen:
                seen.add(key)
                problems.append((where["file_name"], where["line_start"], "does not compile: " + message["message"].splitlines()[0]))
            continue
        code = (message.get("code") or {}).get("code")
        if code != "clippy::disallowed_methods":
            continue
        found = DISALLOWED.match(message["message"])
        path = found.group(1) if found else message["message"]
        chain = expansions(where)
        if not chain:
            key = ("plain", where["file_name"], where["byte_start"], path)
            if key in seen:
                continue
            seen.add(key)
            plain[(where["file_name"], path)] += 1
            continue
        inner = chain[0]
        call = inner["span"]
        definition = inner.get("def_site_span") or {}
        key = ("macro", call["file_name"], call["byte_start"], path)
        if key in seen:
            continue
        seen.add(key)
        at = (call["file_name"], call["line_start"])
        def_file = definition.get("file_name")
        def_sites[def_file].add(definition.get("line_start"))
        if where["file_name"] != def_file:
            problems.append((*at, f"{path} in an argument of {inner['macro_decl_name']}, not in its body"))
        elif def_file == exception_file and path in ENV_READERS:
            names = invocation_names(crate, call)
            if names is None:
                problems.append((*at, "an env_exception! that is not the literal form"))
                continue
            invocations.append((call["file_name"], names, call["line_start"], call["byte_start"]))
        elif def_file == process_env_file and path in ENV_READERS and inner["macro_decl_name"].endswith("process_env!"):
            process_env += 1
        else:
            problems.append((*at, f"{path} inside {inner['macro_decl_name']} (defined in {def_file})"))
    for def_file, lines in def_sites.items():
        if def_file in (exception_file, process_env_file) and len(lines) != 1:
            problems.append((def_file, 0, f"{len(lines)} macros there let a read through, not one"))
    return invocations, process_env, plain, problems


def judge(invocations, plain, problems, exceptions, plain_listed):
    """Problems, the tables compared both ways."""
    problems = list(problems)
    for file, names, line, _ in invocations:
        for name in names:
            if name not in ALLOWED_NAMES:
                problems.append((file, line, f"env_exception! reads {name}, which is not an allowed name"))
    wanted = collections.Counter(exceptions)
    have = collections.Counter((file, names) for file, names, _, _ in invocations)
    for (file, names), count in (have - wanted).items():
        lines = [line for f, n, line, _ in invocations if (f, n) == (file, names)]
        problems.append((file, lines[0], f"an unlisted env_exception! of {list(names)} ({count} more than listed)"))
    for (file, names), count in (wanted - have).items():
        problems.append((file, 0, f"a listed env_exception! of {list(names)} is not there ({count})"))
    listed = collections.Counter({(file, path): count for file, path, count in plain_listed})
    for (file, path), count in (plain - listed).items():
        problems.append((file, 0, f"{path} outside env_exception! and process_env!, unlisted ({count})"))
    for (file, path), count in (listed - plain).items():
        problems.append((file, 0, f"a listed {path} is not there ({count})"))
    return problems


def check_tree():
    invocations, process_env, plain, problems = [], 0, collections.Counter(), []
    seen_invocations = set()
    for features in ([], ["--features", "fault-injection"]):
        status, finished, stderr, messages = clippy(ROOT, ["--locked"] + features)
        if status != 0 or finished is not True:
            sys.stderr.write(stderr[-4000:])
            print(f"check-env-reads: clippy {' '.join(features) or '(default features)'} exited {status}")
            return 1
        found, arms, calls, wrong = classify(ROOT, messages, ENV_EXCEPTION_FILE, PROCESS_ENV_FILE)
        for invocation in found:
            key = (invocation[0], invocation[3])
            if key not in seen_invocations:
                seen_invocations.add(key)
                invocations.append(invocation)
        process_env = max(process_env, arms)
        plain = plain | calls
        problems += [problem for problem in wrong if problem not in problems]
    problems = judge(invocations, plain, problems, EXCEPTIONS, PLAIN)
    used = {name for _, names, _, _ in invocations for name in names}
    for name in sorted(set(ALLOWED_NAMES) - used):
        problems.append(("scripts/check-env-reads.py", 0, f"ALLOWED_NAMES holds {name}, which nothing reads"))
    print(
        f"check-env-reads: {len(invocations)} env_exception! invocations in "
        f"{len({file for file, _, _, _ in invocations})} files, {process_env} process_env! "
        f"build arms, {sum(plain.values())} listed plain calls"
    )
    for file, line, why in sorted(problems):
        print(f"  refused: {file}:{line}: {why}")
    if problems:
        print(f"check-env-reads: {len(problems)} refused")
        return 1
    print("check-env-reads: OK")
    return 0


# The fixtures: each way round the rule the reviews of the old text scanner
# found, and each way to lift a lint without naming it. Every one is a test
# file of a crate that compiles src/env_exception.rs itself, under xfx's own
# clippy.toml; each must be refused for the reason given, and the one accepted
# shape accepted.
ALLOW = '#[allow(clippy::disallowed_methods, reason = "env: TMUX -- the fixture")]'
FIXTURES = {
    "accepted": ('pub fn f() -> bool { env_fixtures::env_exception!(var_os("PATH")).is_some() }', []),
    "multi_line_reason": (
        '#[allow(\n    clippy::disallowed_methods,\n    reason = "env: TMUX -- a reason\n    over two lines"\n)]\n'
        'pub fn f() -> bool { std::env::var_os("HOME").is_some() }',
        ["std::env::var_os outside"],
    ),
    "let_cut_at_a_brace": (
        "pub fn f() -> usize {\n    " + ALLOW + "\n    let width = {\n        0\n    }\n"
        '    .max(std::env::var_os("HOME").map_or(0, |_| 1));\n    width\n}',
        ["std::env::var_os outside"],
    ),
    "quote_char_unit": (
        "pub fn f() -> bool {\n    " + ALLOW + "\n    let (_, home) = ('\"', std::env::var_os(\"HOME\"));\n    home.is_some()\n}",
        ["std::env::var_os outside"],
    ),
    "renamed_import": (
        "mod inner {\n    pub(crate) use std::env::var_os as v;\n}\n"
        "pub fn f() -> bool {\n    " + ALLOW + '\n    let home = inner::v("HOME");\n    home.is_some()\n}',
        ["std::env::var_os outside"],
    ),
    "braced_import": (
        "use std::env::{self as e, var_os as v};\n"
        "pub fn f() -> bool {\n    " + ALLOW + '\n    let both = (v("HOME"), e::var("HOME"));\n    both.0.is_some()\n}',
        ["std::env::var_os outside", "std::env::var outside"],
    ),
    "closure_rebinding": (
        "pub fn f() -> bool {\n    let read = |name: &str| {\n        " + ALLOW + "\n        let value = std::env::var_os(name);\n        value\n    };\n"
        '    read("HOME").is_some()\n}',
        ["std::env::var_os outside"],
    ),
    "shadowed_constant": (
        'const NAME: &str = "TMUX";\npub fn f() -> bool {\n    const NAME: &str = "HOME";\n    ' + ALLOW
        + "\n    let home = std::env::var_os(NAME);\n    home.is_some()\n}",
        ["std::env::var_os outside"],
    ),
    "alias_through_alias": (
        "use std::env as e;\nuse e::var_os as v;\npub fn f() -> bool {\n    " + ALLOW + '\n    let home = v("HOME");\n    home.is_some()\n}',
        ["std::env::var_os outside"],
    ),
    "path_reference": (
        'pub fn f() -> bool {\n    let read = std::env::var_os;\n    read("HOME").is_some()\n}',
        ["std::env::var_os outside"],
    ),
    "libc_system_as_run": (
        "use libc::system as run;\npub fn f() -> i32 {\n    " + ALLOW + '\n    let status = unsafe { run(c"true".as_ptr()) };\n    status\n}',
        ["libc::system outside"],
    ),
    "every_reader": (
        "pub fn f() -> usize {\n    " + ALLOW + "\n    let all = (\n"
        '        std::env::var("A").is_ok(),\n        std::env::var_os("B").is_some(),\n'
        "        std::env::vars().count(),\n        std::env::vars_os().count(),\n"
        "        std::env::home_dir().is_some(),\n"
        '        unsafe { libc::getenv(c"C".as_ptr()) }.is_null(),\n    );\n    all.2 + all.3\n}',
        ["std::env::var outside", "std::env::var_os outside", "std::env::vars outside",
         "std::env::vars_os outside", "std::env::home_dir outside", "libc::getenv outside"],
    ),
    "crate_level_allow_all": (
        "#![allow(clippy::all)]\n#![allow(warnings)]\n" + 'pub fn f() -> bool { std::env::var_os("HOME").is_some() }',
        ["std::env::var_os outside"],
    ),
    "group_allow": (
        "#[allow(clippy::style)]\n" + 'pub fn f() -> bool { std::env::var_os("HOME").is_some() }',
        ["std::env::var_os outside"],
    ),
    "expect": (
        "#[expect(clippy::disallowed_methods)]\n" + 'pub fn f() -> bool { std::env::var_os("HOME").is_some() }',
        ["std::env::var_os outside"],
    ),
    "cfg_attr_allow": (
        "#[cfg_attr(all(), allow(clippy::disallowed_methods))]\n" + 'pub fn f() -> bool { std::env::var_os("HOME").is_some() }',
        ["std::env::var_os outside"],
    ),
    "a_start_under_an_env_allow": (
        "pub fn f() -> bool {\n    " + ALLOW + '\n    let started = std::process::Command::new("true").status();\n    started.is_ok()\n}',
        ["std::process::Command::status outside"],
    ),
    "unlisted_name": ('pub fn f() -> bool { env_fixtures::env_exception!(var_os("HOME")).is_some() }', ["not an allowed name"]),
    "unlisted_invocation": ('pub fn f() -> bool { env_fixtures::env_exception!(var_os("TMPDIR")).is_some() }', ["an unlisted env_exception!"]),
    "raw_string_name": ('pub fn f() -> bool { env_fixtures::env_exception!(var_os(r"PATH")).is_some() }', ["not the literal form"]),
    "a_constant_into_the_macro": (
        'const NAME: &str = "PATH";\npub fn f() -> bool { env_fixtures::env_exception!(var_os(NAME)).is_some() }',
        ["does not compile"],
    ),
    "a_macro_of_the_same_name": (
        "macro_rules! env_exception {\n    ($name:expr) => {\n        match () {\n"
        "            #[allow(clippy::disallowed_methods)]\n            () => ::std::env::var_os($name),\n        }\n    };\n}\n"
        'pub fn f() -> bool { env_exception!("HOME").is_some() }',
        ["inside env_exception!"],
    ),
    "a_wrapping_macro": (
        "macro_rules! wrap {\n    ($name:tt) => {\n        env_fixtures::env_exception!(var_os($name))\n    };\n}\n"
        'pub fn f() -> bool { wrap!("PATH").is_some() }',
        ["not the literal form"],
    ),
}


def lock_version(name):
    with open(os.path.join(ROOT, "Cargo.lock")) as lock:
        text = lock.read()
    found = re.search(r'\[\[package\]\]\nname = "' + re.escape(name) + r'"\nversion = "([^"]+)"', text)
    return found.group(1)


def self_test():
    scratch = tempfile.mkdtemp(prefix="check-env-reads-")
    try:
        crate = os.path.join(scratch, "env_fixtures")
        os.makedirs(os.path.join(crate, "src"))
        os.makedirs(os.path.join(crate, "tests"))
        with open(os.path.join(crate, "Cargo.toml"), "w") as manifest:
            manifest.write(
                '[package]\nname = "env_fixtures"\nversion = "0.0.0"\nedition = "2021"\npublish = false\n\n'
                f'[dependencies]\nlibc = "={lock_version("libc")}"\n'
            )
        shutil.copy(os.path.join(ROOT, "clippy.toml"), crate)
        shutil.copy(os.path.join(ROOT, "Cargo.lock"), crate)
        macro = os.path.join(ROOT, ENV_EXCEPTION_FILE)
        with open(os.path.join(crate, "src", "lib.rs"), "w") as lib:
            lib.write(f"#[macro_use]\n#[path = {json.dumps(macro)}]\nmod env_exception;\n")
        for name, (source, _) in FIXTURES.items():
            with open(os.path.join(crate, "tests", name + ".rs"), "w") as fixture:
                fixture.write(source + "\n")
        _, _, stderr, messages = clippy(
            crate, ["--offline", "--keep-going"], {"CARGO_TARGET_DIR": os.path.join(scratch, "target")}
        )
        invocations, _, plain, problems = classify(crate, messages, macro, None)
        problems = judge(invocations, plain, problems, [("tests/accepted.rs", ("PATH",))], [])
        failures = 0
        for name, (_, reasons) in FIXTURES.items():
            file = f"tests/{name}.rs"
            mine = [why for f, _, why in problems if f == file]
            if not reasons:
                verdict = "accepted" if not mine else "REFUSED, expected accepted"
                failures += bool(mine)
            else:
                missing = [reason for reason in reasons if not any(reason in why for why in mine)]
                verdict = "refused" if not missing else f"NOT REFUSED for {missing}"
                failures += bool(missing)
            print(f"{verdict:9} {file}" + "".join(f"\n            {why}" for why in mine))
        if not messages:
            sys.stderr.write(stderr[-4000:])
            failures += 1
        print(f"check-env-reads --self-test: {len(FIXTURES)} fixtures, {failures} not as expected")
        return 1 if failures else 0
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        sys.exit(self_test())
    if sys.argv[1:]:
        sys.exit("usage: check-env-reads.py [--self-test]")
    sys.exit(check_tree())
