#!/usr/bin/env python3
"""Every read of the process environment, every start of a child, and every
test binary, against what clippy.toml and cargo say.

    python3 scripts/check-env-reads.py              # the tree
    python3 scripts/check-env-reads.py --self-test  # the fixtures below

The check runs clippy itself, with `clippy::disallowed_methods` and
`clippy::disallowed_macros` forced on (`--force-warn`): no `allow`, `expect`,
group, `warnings`, crate-level attribute, `cfg_attr`, `-A` or `[lints]`
table can turn a forced lint off, so every call the lints know of is
reported, whatever lets it through in the ordinary run. Clippy resolves each
call's path, so an import, an alias, a closure or a function reference is
the call it names. It does so in the dev and release profiles, each with the
default and with all features (PASSES), one union of reports. Nothing here
reads source text to find a read: the reports are clippy's, and the only
source bytes read are the compiler's own span of an invocation.

Each reported call is refused unless it is one of:

- an `env_exception!` arm (src/env_exception.rs): its invocation has exactly
  the literal form, its names are in ALLOWED_NAMES, and its file and names
  are an entry of EXCEPTIONS -- every invocation, compared both ways;
- a `process_env!` build arm (src/lib.rs): names computed by design, read
  from the process in a build and from the case's fake process in a unit
  test (src/test_env.rs);
- an `env!` or `option_env!` of a literal name in COMPILE_TIME_NAMES;
- a call outside any macro listed in PLAIN, by file, path and count; one in
  tests/ only in a test binary that runs in a clean copy of itself.

Beside clippy's reports: rustc's dep-info names every compile-time read of
every crate built from this manifest, held to COMPILE_TIME_NAMES; clippy.toml
must hold every reader, start and macro listed here, and no .clippy.toml may
shadow it; and every test executable cargo builds (TEST_BUILDS) is started,
to show it refuses to run a case outside its clean copy
(artifact_problems).

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
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The readers clippy.toml disallows for the environment.
ENV_READERS = {
    "std::env::var",
    "std::env::var_os",
    "std::env::vars",
    "std::env::vars_os",
    "std::env::home_dir",
    "std::env::temp_dir",
    "libc::getenv",
    "libc::_NSGetEnviron",
    # The user database: the home a user is given, with no read of HOME --
    # what `home_dir` falls back on.
    "libc::getpwuid",
    "libc::getpwuid_r",
    "libc::getpwnam",
    "libc::getpwnam_r",
    "libc::getpwent",
    "libc::getpwent_r",
}

# The start rule's entries: what makes or starts a child, replaces the
# process, or makes a pipe or socket pair a start could hand on.
STARTS = {
    "std::process::Command::new",
    "tokio::process::Command::new",
    "std::process::Command::spawn",
    "std::process::Command::status",
    "std::process::Command::output",
    "tokio::process::Command::spawn",
    "tokio::process::Command::status",
    "tokio::process::Command::output",
    "std::os::unix::process::CommandExt::exec",
    "libc::execv",
    "libc::execve",
    "libc::execvp",
    "libc::execvpe",
    "libc::fexecve",
    "libc::execl",
    "libc::execle",
    "libc::execlp",
    "libc::execvP",
    "libc::execveat",
    "libc::syscall",
    "libc::clone",
    "libc::fork",
    "libc::vfork",
    "libc::forkpty",
    "libc::posix_spawn",
    "libc::posix_spawnp",
    "libc::popen",
    "libc::system",
    "libc::pipe",
    "rustix::pipe::pipe",
    "rustix::pipe::pipe_with",
    "std::io::pipe",
    "mio::unix::pipe::new",
    "libc::socketpair",
    "std::os::unix::net::UnixStream::pair",
    "std::os::unix::net::UnixDatagram::pair",
}

# Every variable an `env_exception!` may name, and why it may: none names the
# person's profile or configuration, an xfx setting, a proxy or a credential,
# and `HOME` is here only for the copy that confirms its own is the empty one
# made for it (EXCEPTIONS holds that read to tests/support/isolated.rs). A
# list of what is allowed, not of what is not: a name nobody thought to
# forbid -- a token another tool reads -- is refused until someone adds it
# here with its reason.
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
    "XFX_TEST_ISOLATED_COPY": "the copy of an integration test binary, and what its home is called",
    "HOME": "only the copy of an integration test binary's, made for it and confirmed empty before any case runs",
    "RUST_BACKTRACE": "libtest's and std's own, handed to that copy",
    "RUST_TEST_THREADS": "libtest's own, handed to that copy",
    "XFX_DRAWN_WIDER_LIST": "the drawn-wider sweep's own",
    "XFX_DRAWN_WIDER_OUT": "the drawn-wider sweep's own",
}

# `env_exception!(names())`: the names alone, never a value.
NAMES = "names()"

# Every name `env!` or `option_env!` may read at compile time, and why: what
# cargo and build.rs set for the crate, never the person's environment.
COMPILE_TIME_NAMES = {
    "CARGO_MANIFEST_DIR": "cargo's: where a test finds the repository's own files",
    "CARGO_PKG_NAME": "Cargo.toml's name",
    "CARGO_PKG_VERSION": "Cargo.toml's version",
    "CARGO_BIN_EXE_xfx": "cargo's: the xfx binary an integration test starts",
    "XFX_BUILD_REVISION": "build.rs's: the revision the binary is built from",
    "XFX_BUILD_CHANNEL": "build.rs's: the channel the build claims",
}

# What clippy-driver itself records as read at compile time (its own
# arguments and configuration directory): not the crate's. Dropped from the
# clippy passes' dep-info only; a `cargo test` build is rustc's alone.
CLIPPY_OWN = {"CLIPPY_ARGS", "CLIPPY_CONF_DIR"}

# What an artifact whose dep-info cannot be found is recorded as reading.
NO_DEP_INFO = "\0no dep-info"

# The compile-time readers clippy.toml disallows as macros.
ENV_MACROS = {"std::env", "std::option_env", "core::env", "core::option_env"}

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
    # The standard library's temporary directory, in three unit tests.
    ("src/tools/terminal.rs", ("TMPDIR",)),
    ("src/mcp/config.rs", ("TMPDIR",)),
    ("src/mcp/command/tests.rs", ("TMPDIR",)),
    ("src/mcp/docker.rs", ("TMPDIR", "TEMP", "TMP")),
    ("tests/support/sandbox.rs", ("PATH",)),
    ("tests/support/sandbox.rs", ("TMPDIR",)),
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
    # Every integration test binary, before main: the one it was started as
    # reads its marker and what it hands its copy; the copy confirms that it
    # was given these names and no other, and the home the marker names.
    ("tests/support/isolated.rs", ("XFX_TEST_ISOLATED_COPY",)),
    ("tests/support/isolated.rs", ("TMPDIR",)),
    ("tests/support/isolated.rs", ("RUST_BACKTRACE",)),
    ("tests/support/isolated.rs", ("RUST_TEST_THREADS",)),
    ("tests/support/isolated.rs", (NAMES,)),
    ("tests/support/isolated.rs", ("HOME",)),
    # An ignored case the drawn-wider sweep runs, with its own two.
    ("src/tui/drawn_wider.rs", ("XFX_DRAWN_WIDER_LIST",)),
    ("src/tui/drawn_wider.rs", ("XFX_DRAWN_WIDER_OUT",)),
    # Set by cargo in a test process: the case shows a command does not
    # inherit it.
    ("tests/permissions.rs", ("CARGO_PKG_NAME",)),
]

# Calls outside any macro that clippy is told to let through: file, path,
# how many. One in tests/ is let through only in a test binary that runs its
# cases in a clean copy of itself (tests/support/isolated.rs).
PLAIN = [
    # The Command every child of xfx's is made from (process::command), the
    # start every other start goes through (process::spawn and
    # process::spawn_unless), and the pipe every other pipe does
    # (process::pipe), under the lock.
    ("src/process.rs", "std::process::Command::new", 1),
    ("src/process.rs", "std::process::Command::spawn", 2),
    ("src/process.rs", "rustix::pipe::pipe", 1),
    # The bare Command a unit test about what a bare Command's child inherits
    # makes, its environment cleared.
    ("src/tui/signals.rs", "std::process::Command::new", 1),
    # The Command every child an integration test starts is made from, and
    # the exec that makes a test binary its clean copy.
    ("tests/support/mod.rs", "std::process::Command::new", 1),
    ("tests/support/isolated.rs", "std::os::unix::process::CommandExt::exec", 1),
    # The build script: a process cargo runs before the crate exists, never a
    # test and never the product. It reads what cargo and a publishing
    # workflow set (XFX_BUILD_REVISION, XFX_BUILD_CHANNEL, PROFILE), drops
    # every GIT_* name from the git it starts, and starts that git (two
    # calls: `git_path` and `git_revision`).
    ("build.rs", "std::env::var", 3),
    ("build.rs", "std::env::vars_os", 1),
    ("build.rs", "std::process::Command::new", 1),
    ("build.rs", "std::process::Command::output", 2),
]

ENV_EXCEPTION_FILE = "src/env_exception.rs"
ISOLATION_FILE = "tests/support/isolated.rs"
PROCESS_ENV_FILE = "src/lib.rs"

NAME = r'"([A-Z][A-Z0-9_]*)"'
SINGLE = re.compile(
    r"^(?:[a-z_][a-z0-9_]*::)?env_exception!\s*\(\s*(?:var|var_os)\s*\(\s*" + NAME + r"\s*\)\s*\)$"
)
NAMES_ONLY = re.compile(r"^(?:[a-z_][a-z0-9_]*::)?env_exception!\s*\(\s*names\s*\(\s*\)\s*\)$")
TEMP_DIR = re.compile(r"^(?:[a-z_][a-z0-9_]*::)?env_exception!\s*\(\s*temp_dir\s*\(\s*\)\s*\)$")
LIST = re.compile(
    r"^(?:[a-z_][a-z0-9_]*::)?env_exception!\s*\(\s*first_set_os\s*\(\s*\[\s*"
    r'((?:"[A-Z][A-Z0-9_]*"\s*,\s*)*"[A-Z][A-Z0-9_]*")\s*,?\s*\]\s*\)\s*\)$'
)
DISALLOWED = re.compile(r"^use of a disallowed (?:method|macro) `([^`]+)`")
COMPILE_TIME = re.compile(
    r'^(?:(?:std|core)::)?(?:option_env|env)!\s*\(\s*"([A-Za-z_][A-Za-z0-9_]*)"\s*(?:,\s*"[^"\\]*"\s*)?,?\s*\)$'
)

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
    env = tool_environment(env_extra)
    command = (
        ["cargo", "clippy", "--all-targets", "--message-format=json"]
        + extra
        + ["--", "--force-warn", "clippy::disallowed_methods", "--force-warn", "clippy::disallowed_macros"]
    )
    run = subprocess.run(
        command, cwd=crate, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    messages = []
    tests = set()
    compiled = collections.defaultdict(set)
    finished = None
    manifest = os.path.realpath(os.path.join(crate, "Cargo.toml"))
    for line in run.stdout.decode().splitlines():
        if not line.startswith("{"):
            continue
        record = json.loads(line)
        if record.get("reason") == "compiler-message":
            messages.append(record)
        elif record.get("reason") == "compiler-artifact":
            if record["target"]["kind"] == ["test"]:
                tests.add(record["target"]["name"])
            if os.path.realpath(record.get("manifest_path", "")) == manifest:
                record_compiled(compiled, record, crate)
        elif record.get("reason") == "build-finished":
            finished = record.get("success")
    for source in compiled:
        if source != SOURCES:
            compiled[source] -= CLIPPY_OWN
    return run.returncode, finished, run.stderr.decode(), messages, tests, compiled


HASHED = re.compile(r"^(?P<lib>lib)?(?P<stem>[A-Za-z0-9_]+-[0-9a-f]{16})(?P<ext>\.(?:rlib|rmeta|so|dylib|a))?$")


LIBRARY_EXTENSIONS = (".rlib", ".rmeta", ".so", ".dylib", ".a")


def dep_info(filename):
    """rustc's dep-info for one of an artifact's files, as (the `.d` file,
    the target lines it must hold), or None.

    rustc writes `<crate>-<hash>.d` beside `lib<crate>-<hash>.rlib` or
    `.rmeta` and beside an executable `<crate>-<hash>` (whose name may begin
    with `lib`: only a library's file name has the prefix to strip). Cargo
    uplifts an executable (`target/debug/xfx`, a build script's
    `build-script-build`) by a hard link or, on macOS, a copy of such a
    file, so an uplifted one is traced back to its original: the one
    candidate with its device and inode, else the one with its bytes; two
    candidates either way is no answer."""
    directory, base = os.path.split(filename)
    hashed = HASHED.match(base)
    if hashed and (hashed.group("ext") or not hashed.group("lib")):
        stem = hashed.group("stem")
        targets = {os.path.join(directory, stem)} | {
            os.path.join(directory, "lib" + stem + extension) for extension in LIBRARY_EXTENSIONS
        }
        return os.path.join(directory, stem + ".d"), targets
    if hashed:
        return os.path.join(directory, base + ".d"), {os.path.join(directory, base)}
    try:
        mine = os.stat(filename)
    except OSError:
        return None
    crate = base.replace("-", "_")
    candidates = []
    for place in (directory, os.path.join(directory, "deps")):
        try:
            names = os.listdir(place)
        except OSError:
            continue
        for name in names:
            found = HASHED.match(name)
            if name.startswith(crate + "-") and found and not found.group("ext") and not found.group("lib"):
                candidates.append(os.path.join(place, name))
    stats = {}
    for candidate in candidates:
        try:
            stats[candidate] = os.stat(candidate)
        except OSError:
            # A dangling link among the candidates: no answer, not a crash.
            return None
    same_file = [c for c in candidates if (stats[c].st_dev, stats[c].st_ino) == (mine.st_dev, mine.st_ino)]
    if not same_file:
        same_file = [c for c in candidates if stats[c].st_size == mine.st_size and same_bytes(c, filename)]
    return dep_info(same_file[0]) if len(same_file) == 1 else None


def same_bytes(one, other):
    with open(one, "rb") as a, open(other, "rb") as b:
        while True:
            left, right = a.read(1 << 20), b.read(1 << 20)
            if left != right:
                return False
            if not left:
                return True


RULE = re.compile(r"^((?:[^:\\\s]|\\.)+):(?:\s+(.*))?$")


def split_paths(text):
    """The paths of a make rule's right-hand side, `\\ ` an escaped space."""
    return [path.replace("\\ ", " ") for path in re.split(r"(?<!\\)\s+", text.strip()) if path]


def read_dep_info(info, targets, root):
    """(the names read at compile time, the source files) from a `.d`, or
    None unless it holds one of `targets` as a rule with sources: an empty,
    a truncated or a foreign file is not evidence."""
    try:
        with open(info, "rb") as lines:
            text = lines.read().decode("utf-8", errors="replace")
    except OSError:
        return None
    names, sources, valid = set(), set(), False
    for line in text.splitlines():
        if line.startswith("# env-dep:"):
            names.add(line[len("# env-dep:") :].split("=", 1)[0].strip())
            continue
        rule = RULE.match(line)
        if not rule:
            continue
        target = rule.group(1).replace("\\ ", " ")
        paths = split_paths(rule.group(2) or "")
        if target in targets and paths:
            valid = True
            sources |= {os.path.normpath(os.path.join(root, path)) for path in paths}
    return (names, sources) if valid else None


def compile_time_names(filenames, root):
    """The names rustc's dep-info for an artifact says were read at compile
    time (`# env-dep:NAME=value`), the names and never the values, and the
    source files it was compiled from. None when no file of the artifact has
    dep-info that is its own: missing evidence is not an empty set of
    reads."""
    names, sources, found = set(), set(), False
    for filename in filenames:
        located = dep_info(filename)
        read = read_dep_info(*located, root) if located else None
        if read is None:
            continue
        found = True
        names |= read[0]
        sources |= read[1]
    return (names, sources) if found else None


# Where `compiled` keeps the source files every compilation read.
SOURCES = "\0sources"


def record_compiled(compiled, record, root):
    """Adds an artifact's compile-time reads to `compiled`, under its source,
    and its source files under SOURCES; an artifact with no dep-info of its
    own is recorded as such."""
    source = os.path.relpath(record["target"]["src_path"], root)
    read = compile_time_names(record["filenames"], root)
    if read is None:
        compiled[source].add(NO_DEP_INFO)
    else:
        compiled[source] |= read[0]
        compiled[SOURCES] |= {(source, path) for path in read[1]}


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
    if NAMES_ONLY.match(text):
        return (NAMES,)
    if TEMP_DIR.match(text):
        return ("TMPDIR",)
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
    plain_targets = collections.defaultdict(set)
    compile_time = collections.Counter()
    seen_at_compile_time = collections.defaultdict(set)
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
        if code not in ("clippy::disallowed_methods", "clippy::disallowed_macros"):
            continue
        found = DISALLOWED.match(message["message"])
        path = found.group(1) if found else message["message"]
        if code == "clippy::disallowed_macros":
            key = ("macro-use", where["file_name"], where["byte_start"])
            if key in seen:
                continue
            seen.add(key)
            at = (where["file_name"], where["line_start"])
            if path not in ENV_MACROS:
                problems.append((*at, f"a use of {path}"))
                continue
            with open(os.path.join(crate, where["file_name"]), "rb") as source:
                text = source.read()[where["byte_start"] : where["byte_end"]].decode()
            literal = COMPILE_TIME.match(text)
            if literal:
                seen_at_compile_time[os.path.relpath(record["target"]["src_path"], crate)].add(literal.group(1))
            if not literal:
                problems.append((*at, f"a {path}! whose name is not a literal"))
            elif literal.group(1) not in COMPILE_TIME_NAMES:
                problems.append((*at, f"a {path}! of {literal.group(1)}, which is not a compile-time name"))
            else:
                compile_time[literal.group(1)] += 1
            continue
        chain = expansions(where)
        if not chain:
            plain_targets[where["file_name"]].add(record["target"]["name"])
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
            shown = def_file if def_file and not os.path.isabs(def_file) else "a file outside this package"
            problems.append((*at, f"{path} inside {inner['macro_decl_name']} (defined in {shown})"))
    for def_file, lines in def_sites.items():
        if def_file in (exception_file, process_env_file) and len(lines) != 1:
            problems.append((def_file, 0, f"{len(lines)} macros there let a read through, not one"))
    return invocations, process_env, plain, problems, plain_targets, compile_time, seen_at_compile_time


def compiled_problems(compiled):
    """A crate rustc says read, at compile time, a name not listed, and one
    built without dep-info to say what it read."""
    problems = []
    for source, names in sorted(compiled.items()):
        if source == SOURCES:
            continue
        if NO_DEP_INFO in names:
            problems.append((source, 0, "built with no dep-info to say what it read at compile time"))
        for name in sorted(names - set(COMPILE_TIME_NAMES) - {NO_DEP_INFO}):
            problems.append((source, 0, f"compiled with {name} from the environment (rustc's dep-info), which is not a compile-time name"))
    return problems


def unrecorded_problems(seen, compiled):
    """A compile-time read clippy's macro lint saw that the same pass's
    dep-info does not hold: a `.d` cut short (rustc writes its `# env-dep:`
    lines last) or rewritten."""
    problems = []
    for source, names in sorted(seen.items()):
        if NO_DEP_INFO in compiled.get(source, ()):
            continue
        for name in sorted(names - compiled.get(source, set())):
            problems.append((source, 0, f"dep-info without {name}, a compile-time read clippy saw: cut short or rewritten"))
    return problems


def judge(invocations, plain, problems, exceptions, plain_listed):
    """Problems, the tables compared both ways."""
    problems = list(problems)
    for file, names, line, _ in invocations:
        for name in names:
            if name not in ALLOWED_NAMES and name != NAMES:
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


# The supported matrix, the same for the clippy passes and the test builds:
# the dev and release profiles, each with no feature and with every feature.
# It is every feature set this manifest can be built with only while it has
# one feature and no default (matrix_problems); a second feature would make
# subsets this does not build.
MATRIX = ([], ["--all-features"], ["--release"], ["--release", "--all-features"])
PASSES = MATRIX
TEST_BUILDS = MATRIX


def matrix_problems(root):
    """MATRIX is every feature set the manifest at `root` has."""
    try:
        import tomllib
    except ImportError:
        return [("Cargo.toml", 0, "no tomllib (Python 3.11 or later) to read it with")]
    with open(os.path.join(root, "Cargo.toml"), "rb") as manifest:
        package = tomllib.load(manifest)
    features = set(package.get("features", {}))
    # An optional dependency is a feature of its own.
    tables = [package] + list(package.get("target", {}).values())
    for table in tables:
        for kind in ("dependencies", "build-dependencies"):
            for name, spec in table.get(kind, {}).items():
                if isinstance(spec, dict) and spec.get("optional"):
                    features.add(name)
    if "default" in features:
        return [("Cargo.toml", 0, "a default feature, which the check's matrix (none, all) never turns off")]
    if len(features) > 1:
        return [("Cargo.toml", 0, "features the check's matrix (none, all) does not cover every set of")]
    return []

# The marker the probe starts each integration test binary with: not a name
# a parent makes, so a binary that runs the constructor refuses itself.
PROBE_MARKER = "not-a-marker"


def artifact_problems(root, cargo_flags=("--locked",), env_extra=None):
    """What cargo builds to run tests, and whether each runs its cases in a
    clean copy of itself.

    For each of TEST_BUILDS, `cargo test --no-run --all-targets` lists every
    executable it built for a test profile. The library's unit-test binary
    is the one that runs in the developer's process by design: every read
    there goes through `process_env!` to the case's fake process
    (src/test_env.rs). Every executable of kind `test` is started with
    `--list` and a marker no parent makes: a binary whose constructor
    (tests/support/isolated.rs) is linked and runs refuses itself before
    main, exit 101; one without it lists its cases. Any other kind (a
    binary's harness, a bench, an example) must list no case at all: `cargo
    test --all-targets` builds the binary's harness even with `test = false`,
    and a harness with nothing in it runs nothing.

    A run, not a symbol table: `cargo test --release` builds with the
    release profile's `strip = true`, so a release test binary has no
    symbols to read, and a run shows the constructor runs, not only that it
    is there. Also returns the names rustc's dep-info records as read at
    compile time for each crate built."""
    problems = []
    compiled = collections.defaultdict(set)
    manifest = os.path.realpath(os.path.join(root, "Cargo.toml"))
    probed = 0
    for build in TEST_BUILDS:
        run = subprocess.run(
            ["cargo", "test", *cargo_flags, "--no-run", "--all-targets", "--message-format=json"] + build,
            cwd=root, env=tool_environment(env_extra), stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        label = " ".join(build) or "(dev, default features)"
        if run.returncode != 0:
            sys.stderr.write(run.stderr.decode()[-4000:])
            problems.append(("Cargo.toml", 0, f"cargo test --no-run {label} exited {run.returncode}"))
            continue
        for line in run.stdout.decode().splitlines():
            if not line.startswith("{"):
                continue
            record = json.loads(line)
            if record.get("reason") != "compiler-artifact":
                continue
            if os.path.realpath(record.get("manifest_path", "")) != manifest:
                continue
            source = os.path.relpath(record["target"]["src_path"], root)
            record_compiled(compiled, record, root)
            if not record["profile"]["test"] or not record.get("executable"):
                continue
            kind = record["target"]["kind"]
            if kind == ["lib"]:
                continue
            if kind != ["test"]:
                listing = subprocess.run(
                    [record["executable"], "--list"], cwd=root, env={"PATH": os.environ.get("PATH", "")},
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120,
                )
                if listing.returncode != 0 or not listing.stdout.rstrip().endswith(b"0 tests, 0 benchmarks"):
                    problems.append((source, 0, f"a {'/'.join(kind)} built to run tests in {label} that holds cases, outside both isolations"))
                continue
            probe = subprocess.run(
                [record["executable"], "--list"],
                cwd=root,
                env={"PATH": os.environ.get("PATH", ""), "XFX_TEST_ISOLATED_COPY": PROBE_MARKER},
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120,
            )
            probed += 1
            if probe.returncode != 101 or b"no case runs" not in probe.stderr:
                problems.append((source, 0, f"its test binary ({label}) runs its cases without making a clean copy of itself (exit {probe.returncode})"))
    return problems, compiled, probed


def tool_environment(extra=None):
    """This process's environment less what changes which lints run, which
    configuration clippy reads, or what wraps the compiler."""
    env = {
        name: value
        for name, value in os.environ.items()
        if name not in DROPPED
        and not (name.startswith("CARGO_TARGET_") and name.endswith("_RUSTFLAGS"))
    }
    env.update(extra or {})
    return env


CFG_CALL = re.compile(r"\bcfg(?P<attr>_attr)?\s*(?P<bang>!)?\s*\(")
CFG_SELECT = re.compile(r"\bcfg_select\s*!\s*[({\[]")
OPENS, CLOSES = "([{", ")]}"
CLIPPY_WORD = re.compile(r"(?<![A-Za-z0-9_:])clippy(?![A-Za-z0-9_])")
RAW_STRING = re.compile(r'(?:b|c)?r(#*)"')

# Files a compilation lists as read that are not Rust: its configuration.
NOT_RUST = {"Cargo.toml", "Cargo.lock", "clippy.toml", ".clippy.toml"}


def code_of(text):
    """`text` with every comment and every string, raw-string, byte-string
    and char literal turned to spaces (newlines kept), so what is left is
    code and every offset still points at the same character. `//` and
    nested `/* */` comments; `"..."`, `b"..."`, `c"..."`, `r#"..."#` and
    their byte and C forms; `'x'`, `'\\''`, `b'x'` -- and a lifetime `'a`
    left as code."""
    out = list(text)
    at, size = 0, len(text)

    def blank(start, end):
        for index in range(start, min(end, size)):
            if out[index] != "\n":
                out[index] = " "

    while at < size:
        char = text[at]
        if text.startswith("//", at):
            end = text.find("\n", at)
            end = size if end < 0 else end
            blank(at, end)
            at = end
        elif text.startswith("/*", at):
            depth, end = 1, at + 2
            while end < size and depth:
                if text.startswith("/*", end):
                    depth, end = depth + 1, end + 2
                elif text.startswith("*/", end):
                    depth, end = depth - 1, end + 2
                else:
                    end += 1
            blank(at, end)
            at = end
        elif (raw := RAW_STRING.match(text, at)) and (at == 0 or not (text[at - 1].isalnum() or text[at - 1] == "_")):
            closing = '"' + raw.group(1)
            end = text.find(closing, raw.end())
            end = size if end < 0 else end + len(closing)
            blank(at, end)
            at = end
        elif char == '"' or (char in "bc" and text.startswith('"', at + 1)
                             and (at == 0 or not (text[at - 1].isalnum() or text[at - 1] == "_"))):
            end = at + (1 if char == '"' else 2)
            while end < size and text[end] != '"':
                end += 2 if text[end] == "\\" else 1
            blank(at, end + 1)
            at = end + 1
        elif char == "'" or (char == "b" and text.startswith("'", at + 1)
                             and (at == 0 or not (text[at - 1].isalnum() or text[at - 1] == "_"))):
            start = at + (1 if char == "'" else 2)
            if text.startswith("\\", start):
                end = text.find("'", start + 2)
            elif start + 1 < size and text[start + 1] == "'":
                end = start + 1
            else:
                end = -1
            if end < 0:
                at += 1
            else:
                blank(at, end + 1)
                at = end + 1
        else:
            at += 1
    return "".join(out)


def compiled_sources(compiled, root, also=()):
    """The source files the compilations in `compiled` read, under `root`
    (or one of `also`), and those outside every one of them."""
    inside, outside = set(), set()
    places = [os.path.realpath(place) for place in (root, *also)]
    for _, path in compiled.get(SOURCES, ()):
        if os.path.basename(path) in NOT_RUST:
            continue
        real = os.path.realpath(path)
        if any(path.startswith(place + os.sep) or real.startswith(place + os.sep) for place in places):
            inside.add(path)
        else:
            outside.add(path)
    return inside, outside


def cfg_problems(root, compiled=None, also=()):
    """No cfg predicate names `clippy` -- in `cfg(..)`, `cfg!(..)`, the
    predicate of `cfg_attr(..)` or an arm's predicate of `cfg_select!`, any
    delimiter -- in any file this package compiles.

    Clippy compiles with `cfg(clippy)` set, so code under `not(clippy)` is
    never linted, and the library's unit-test binary would run it in the
    developer's process. The files are every `.rs` under src/ and tests/,
    build.rs, and every source a compilation's dep-info lists -- a `#[path]`
    module, an `include!`d file, one under a linked directory, one a build
    script generates -- and a source outside the repository is refused
    outright. Each is read as code: comments and literals are blanked
    first (code_of). A cfg a macro builds from its arguments is beyond
    this."""
    problems = []
    files = {os.path.join(root, "build.rs")} if os.path.exists(os.path.join(root, "build.rs")) else set()
    for top in ("src", "tests"):
        for directory, _, names in os.walk(os.path.join(root, top), followlinks=True):
            files |= {os.path.join(directory, name) for name in names if name.endswith(".rs")}
    if compiled is not None:
        inside, outside = compiled_sources(compiled, root, also)
        files |= inside
        for path in sorted(outside):
            problems.append((os.path.relpath(path, root), 0, "a source this package compiles from outside the repository"))
    seen = set()
    for path in sorted(files):
        real = os.path.realpath(path)
        if real in seen or not os.path.isfile(real):
            continue
        seen.add(real)
        with open(path, encoding="utf-8", errors="replace") as source:
            text = code_of(source.read())
        for call in CFG_CALL.finditer(text):
            depth, at, predicate_end = 1, call.end(), None
            while at < len(text) and depth:
                char = text[at]
                if char == "(":
                    depth += 1
                elif char == ")":
                    depth -= 1
                elif char == "," and depth == 1 and call.group("attr") and predicate_end is None:
                    predicate_end = at
                at += 1
            predicate = text[call.end() : predicate_end if predicate_end is not None else at - 1]
            if CLIPPY_WORD.search(predicate):
                line = text.count("\n", 0, call.start()) + 1
                problems.append((os.path.relpath(path, root), line, "a cfg predicate that names clippy, under which code is never linted"))
        for start, predicate in cfg_select_predicates(text):
            if CLIPPY_WORD.search(predicate):
                line = text.count("\n", 0, start) + 1
                problems.append((os.path.relpath(path, root), line, "a cfg_select! arm whose predicate names clippy, under which code is never linted"))
    return problems


def closing(text, at):
    """Where the bracket group that opens at `at` ends: the index past its
    closing bracket, any of the three kinds nesting."""
    depth = 0
    while at < len(text):
        if text[at] in OPENS:
            depth += 1
        elif text[at] in CLOSES:
            depth -= 1
            if depth == 0:
                return at + 1
        at += 1
    return len(text)


def cfg_select_predicates(text):
    """Each `cfg_select!` arm's predicate -- the text before its `=>`, at
    the top level of the body, whatever the delimiter -- with where the
    macro starts. The arms' bodies, a braced group or an expression up to
    the next top-level `,`, are skipped: an `allow(clippy::x)` there is not
    a predicate."""
    for found in CFG_SELECT.finditer(text):
        body_end = closing(text, found.end() - 1) - 1
        at = found.end()
        while at < body_end:
            depth, arrow = 0, None
            for index in range(at, body_end):
                char = text[index]
                if char in OPENS:
                    depth += 1
                elif char in CLOSES:
                    depth -= 1
                elif depth == 0 and text.startswith("=>", index):
                    arrow = index
                    break
            if arrow is None:
                break
            yield found.start(), text[at:arrow]
            at = arrow + 2
            while at < body_end and text[at].isspace():
                at += 1
            if at < body_end and text[at] == "{":
                at = closing(text, at)
                while at < body_end and text[at].isspace():
                    at += 1
                if at < body_end and text[at] == ",":
                    at += 1
            else:
                depth = 0
                while at < body_end and not (depth == 0 and text[at] == ","):
                    depth += text[at] in OPENS
                    depth -= text[at] in CLOSES
                    at += 1
                at += 1


def build_script_problems(root, compiled):
    """build.rs, and what it includes, prints no `rustc-cfg`: a cfg the
    build script sets can differ between the check's clippy and a test
    build without naming clippy."""
    problems = []
    files = {os.path.join(root, "build.rs")} if os.path.exists(os.path.join(root, "build.rs")) else set()
    files |= {path for source, path in compiled.get(SOURCES, ()) if source == "build.rs"}
    for path in sorted(files):
        try:
            with open(path, "rb") as script:
                raw = script.read()
        except OSError:
            continue
        if b"rustc-cfg" in raw:
            problems.append((os.path.relpath(path, root), 0, "the build script sets a cfg (rustc-cfg), which can differ between clippy and a test build"))
    return problems


def config_problems(root):
    """clippy.toml is the configuration clippy reads, and it disallows every
    reader and every start this script knows of."""
    problems = []
    if os.path.exists(os.path.join(root, ".clippy.toml")):
        problems.append((".clippy.toml", 0, "a .clippy.toml, which clippy reads in place of clippy.toml"))
    try:
        import tomllib
    except ImportError:
        return problems + [("clippy.toml", 0, "no tomllib (Python 3.11 or later) to read it with")]
    with open(os.path.join(root, "clippy.toml"), "rb") as config:
        rules = tomllib.load(config)
    methods = {entry["path"] for entry in rules.get("disallowed-methods", []) if isinstance(entry, dict)}
    for path in sorted((ENV_READERS | STARTS) - methods):
        problems.append(("clippy.toml", 0, f"disallowed-methods no longer holds {path}"))
    macros = {entry["path"] for entry in rules.get("disallowed-macros", []) if isinstance(entry, dict)}
    for path in sorted(ENV_MACROS - macros):
        problems.append(("clippy.toml", 0, f"disallowed-macros no longer holds {path}"))
    return problems


def check_tree():
    invocations, process_env, plain = [], 0, collections.Counter()
    problems = config_problems(ROOT) + matrix_problems(ROOT)
    compile_time = collections.Counter()
    everything = collections.defaultdict(set)
    seen_invocations = set()
    for build in PASSES:
        status, finished, stderr, messages, tests, compiled = clippy(ROOT, ["--locked"] + build)
        if status != 0 or finished is not True:
            sys.stderr.write(stderr[-4000:])
            print(f"check-env-reads: clippy {' '.join(build) or '(dev, default features)'} exited {status}")
            return 1
        found, arms, calls, wrong, plain_targets, uses, seen = classify(ROOT, messages, ENV_EXCEPTION_FILE, PROCESS_ENV_FILE)
        compile_time = compile_time | uses
        wrong += compiled_problems(compiled) + unrecorded_problems(seen, compiled)
        for source, read in compiled.items():
            everything[source] |= read
        # Every integration test binary reads its environment in
        # support::isolated, before main: one that does not has escaped it.
        isolating = {
            record["target"]["name"]
            for record in messages
            if any(
                expansion["span"]["file_name"] == ISOLATION_FILE
                for span in record["message"]["spans"]
                for expansion in expansions(span)[:1]
            )
        }
        for name in sorted(tests - isolating):
            wrong.append((f"tests/{name}.rs", 0, "a test binary that does not run its cases in a clean copy (support::isolated)"))
        for file, targets in sorted(plain_targets.items()):
            for target in sorted(targets - isolating if file.startswith("tests/") else set()):
                wrong.append((file, 0, f"a start let through in tests/ compiled into {target}, which does not run in a clean copy"))
        for invocation in found:
            key = (invocation[0], invocation[3])
            if key not in seen_invocations:
                seen_invocations.add(key)
                invocations.append(invocation)
        process_env = max(process_env, arms)
        plain = plain | calls
        problems += [problem for problem in wrong if problem not in problems]
    built, compiled, probed = artifact_problems(ROOT)
    problems += built + compiled_problems(compiled)
    for source, read in compiled.items():
        everything[source] |= read
    problems += cfg_problems(ROOT, everything) + build_script_problems(ROOT, everything)
    problems = judge(invocations, plain, problems, EXCEPTIONS, PLAIN)
    used = {name for _, names, _, _ in invocations for name in names}
    for name in sorted(set(ALLOWED_NAMES) - used):
        problems.append(("scripts/check-env-reads.py", 0, f"ALLOWED_NAMES holds {name}, which nothing reads"))
    print(
        f"check-env-reads: {len(invocations)} env_exception! invocations in "
        f"{len({file for file, _, _, _ in invocations})} files, {process_env} process_env! "
        f"call sites (over its build arms), {sum(plain.values())} listed plain calls, "
        f"{sum(compile_time.values())} compile-time reads of {len(compile_time)} names; "
        f"{probed} test binaries started with a marker no parent makes"
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
    "aliased_command": (
        "use std::process::Command as Git;\n"
        'pub fn f() -> std::process::Command { Git::new("git") }',
        ["std::process::Command::new outside"],
    ),
    "command_new_as_a_function": (
        'pub fn f() -> Vec<std::process::Command> { ["git"].iter().map(std::process::Command::new).collect() }',
        ["std::process::Command::new outside"],
    ),
    "command_new_by_qualified_path": (
        'pub fn f() -> std::process::Command { <std::process::Command>::new("git") }',
        ["std::process::Command::new outside"],
    ),
    "exec_and_execv": (
        "use std::os::unix::process::CommandExt;\n"
        "pub fn f(mut command: std::process::Command) -> std::io::Error { command.exec() }\n"
        "/// # Safety\n/// The fixture is never called.\n"
        "pub unsafe fn g() -> i32 { libc::execv(std::ptr::null(), std::ptr::null()) }",
        ["std::os::unix::process::CommandExt::exec outside", "libc::execv outside"],
    ),
    "option_env_of_a_key": (
        "#[cfg(test)]\nmod tests {\n    #[test]\n    fn reads() {\n"
        '        assert!(option_env!("XFX_GATEWAY_KEY").is_none() || true);\n    }\n}',
        ["core::option_env! of XFX_GATEWAY_KEY", "compiled with XFX_GATEWAY_KEY"],
    ),
    "env_inside_concat": (
        'pub const BY: &str = concat!("by ", env!("CARGO_PKG_AUTHORS"));',
        ["compiled with CARGO_PKG_AUTHORS"],
    ),
    "temp_dir_read": (
        'pub fn f() -> std::path::PathBuf { std::env::temp_dir() }',
        ["std::env::temp_dir outside"],
    ),
    "ns_get_environ": (
        "/// # Safety\n/// The fixture is never called.\n"
        '#[cfg(target_vendor = "apple")]\npub unsafe fn f() -> bool { libc::_NSGetEnviron().is_null() }',
        ["libc::_NSGetEnviron outside"] if sys.platform == "darwin" else [],
    ),
    "cfg_not_clippy": (
        "#[cfg(all(test, not(clippy)))]\nmod t {\n    #[test]\n    fn f() {\n"
        '        let _ = std::env::var_os("HOME");\n    }\n}',
        ["a cfg predicate that names clippy"],
    ),
    "cfg_attr_not_clippy": (
        "#[cfg_attr(\n    not(clippy),\n    allow(unused)\n)]\npub fn f() {}",
        ["a cfg predicate that names clippy"],
    ),
    "cfg_macro_clippy": (
        'pub fn f() -> bool { !cfg!(clippy) && std::env::var_os("HOME").is_some() }',
        ["a cfg predicate that names clippy", "std::env::var_os outside"],
    ),
    "cfg_attr_naming_a_clippy_lint": (
        "#[cfg_attr(test, allow(clippy::needless_return))]\npub fn f() {}",
        [],
    ),
    "exec_v_capital_p": (
        "/// # Safety\n/// The fixture is never called.\n"
        '#[cfg(target_vendor = "apple")]\npub unsafe fn f() -> i32 {\n'
        "    libc::execvP(std::ptr::null(), std::ptr::null(), std::ptr::null())\n}",
        ["libc::execvP outside"] if sys.platform == "darwin" else [],
    ),
    "raw_syscall": (
        "/// # Safety\n/// The fixture is never called.\npub unsafe fn f() {\n    let _ = libc::syscall(20);\n}",
        ["libc::syscall outside"],
    ),
    "user_database": (
        "/// # Safety\n/// The fixture is never called.\n"
        "pub unsafe fn f() -> bool {\n    libc::getpwuid(libc::getuid()).is_null()\n}",
        ["libc::getpwuid outside"],
    ),
    "cfg_comment_holding_a_paren": (
        "#[cfg(/*)*/ all(test, not(clippy)))]\nmod t {}",
        ["a cfg predicate that names clippy"],
    ),
    "cfg_comment_before_its_paren": (
        "#[cfg /* c */ (not(clippy))]\nmod t {}",
        ["a cfg predicate that names clippy"],
    ),
    "cfg_string_holding_a_paren": (
        '#[cfg(any(feature = "))", not(clippy)))]\nmod t {}',
        ["a cfg predicate that names clippy"],
    ),
    "cfg_after_a_block_comment_line": (
        "/*\n// */ #[cfg(all(test, not(clippy)))]\nmod t {}",
        ["a cfg predicate that names clippy"],
    ),
    "cfg_in_a_comment_or_a_string": (
        '// cfg(not(clippy))\n/* cfg(not(clippy)) */\n/* /* */ cfg(not(clippy)) */\n'
        'pub const S: &str = "cfg(not(clippy))";\n'
        'pub const R: &str = r#"cfg(not(clippy))"#;\npub const B: &[u8] = b")";\npub const C: char = \'(\';',
        [],
    ),
    "cfg_after_a_lifetime": (
        "pub fn f<'a>(x: &'a u8) -> &'a u8 {\n    #[cfg(not(clippy))]\n    let _ = 'b';\n    x\n}",
        ["a cfg predicate that names clippy"],
    ),
    "cfg_select_braces": (
        "cfg_select! {\n    clippy => {}\n    _ => { pub fn h() {} }\n}",
        ["a cfg_select! arm whose predicate names clippy"],
    ),
    "cfg_select_parentheses": (
        "cfg_select!(\n    clippy => {}\n    _ => { pub fn h() {} }\n);",
        ["a cfg_select! arm whose predicate names clippy"],
    ),
    "cfg_select_brackets": (
        "cfg_select![\n    clippy => {}\n    _ => { pub fn h() {} }\n];",
        ["a cfg_select! arm whose predicate names clippy"],
    ),
    "cfg_select_expression_arms": (
        "pub fn g() -> u8 {\n    cfg_select! { unix => 1, not(clippy) => 2, _ => 3 }\n}",
        ["a cfg_select! arm whose predicate names clippy"],
    ),
    "cfg_select_naming_a_clippy_lint_in_an_arm": (
        "cfg_select! {\n    unix => { #[allow(clippy::needless_return)] pub fn h() {} }\n    _ => { pub fn h() {} }\n}",
        [],
    ),
    "path_module_not_rs": ('#[path = "t.inc"]\nmod t;', ["a cfg predicate that names clippy"]),
    "include_outside_src_and_tests": ('include!("../gen/hidden.rs");', ["a cfg predicate that names clippy"]),
    "under_a_linked_directory": ('#[path = "linked/x.rs"]\nmod x;', ["a cfg predicate that names clippy"]),
    "path_outside_the_repository": ('#[path = "../../outside/y.rs"]\nmod y;', ["from outside the repository"]),
    "user_database_walk": (
        "/// # Safety\n/// The fixture is never called.\n"
        "pub unsafe fn f() -> bool {\n    libc::getpwent().is_null()\n}",
        ["libc::getpwent outside"],
    ),
    "dependency_macro": (
        "pub fn f() -> std::process::Command {\n    macro_dep::start!()\n}",
        ["std::process::Command::new inside"],
    ),
    "release_only_read": (
        "#[cfg(not(debug_assertions))]\n" + 'pub fn f() -> bool { std::env::var_os("HOME").is_some() }',
        ["std::env::var_os outside"],
    ),
    "a_start_under_an_env_allow": (
        "pub fn f() -> bool {\n    " + ALLOW + '\n    let started = std::process::Command::new("true").status();\n    started.is_ok()\n}',
        ["std::process::Command::status outside"],
    ),
    "unlisted_name": ('pub fn f() -> bool { env_fixtures::env_exception!(var_os("GIT_CONFIG_GLOBAL")).is_some() }', ["not an allowed name"]),
    "home_outside_the_copy": ('pub fn f() -> bool { env_fixtures::env_exception!(var_os("HOME")).is_some() }', ["an unlisted env_exception! of ['HOME']"]),
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


# Files the fixtures above compile beyond their own: by path under the fixture
# crate, or, with "../", beside it -- outside the repository it stands for.
HIDDEN = "#[cfg(not(clippy))]\npub fn hidden() {}\n"
FIXTURE_FILES = {
    "tests/t.inc": HIDDEN,
    "gen/hidden.rs": HIDDEN,
    "linked_target/x.rs": HIDDEN,
    "../outside/y.rs": "pub fn outside() {}\n",
}
# Which fixture a refusal in one of those files belongs to.
FIXTURE_OWNS = {
    "path_module_not_rs": ("tests/t.inc",),
    "include_outside_src_and_tests": ("gen/hidden.rs",),
    "under_a_linked_directory": ("tests/linked/x.rs",),
    "path_outside_the_repository": ("../outside/y.rs",),
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
        # A dependency whose exported macro starts a child: clippy must see
        # into an external macro's expansion.
        dependency = os.path.join(scratch, "macro_dep")
        os.makedirs(os.path.join(dependency, "src"))
        with open(os.path.join(dependency, "Cargo.toml"), "w") as manifest:
            manifest.write('[package]\nname = "macro_dep"\nversion = "0.0.0"\nedition = "2021"\npublish = false\n')
        with open(os.path.join(dependency, "src", "lib.rs"), "w") as lib:
            lib.write('#[macro_export]\nmacro_rules! start {\n    () => {\n        ::std::process::Command::new("git")\n    };\n}\n')
        with open(os.path.join(crate, "Cargo.toml"), "w") as manifest:
            manifest.write(
                '[package]\nname = "env_fixtures"\nversion = "0.0.0"\nedition = "2021"\npublish = false\n\n'
                f'[dependencies]\nlibc = "={lock_version("libc")}"\nmacro_dep = {{ path = "../macro_dep" }}\n\n'
                # As the tree has it: off in the ordinary run, so a fixture
                # is refused only if the check forces the lint on.
                '[lints.clippy]\ndisallowed_macros = "allow"\n'
            )
        shutil.copy(os.path.join(ROOT, "clippy.toml"), crate)
        shutil.copy(os.path.join(ROOT, "Cargo.lock"), crate)
        macro = os.path.join(ROOT, ENV_EXCEPTION_FILE)
        with open(os.path.join(crate, "src", "lib.rs"), "w") as lib:
            lib.write(f"#[macro_use]\n#[path = {json.dumps(macro)}]\nmod env_exception;\n")
        for name, (source, _) in FIXTURES.items():
            with open(os.path.join(crate, "tests", name + ".rs"), "w") as fixture:
                fixture.write(source + "\n")
        # The files three fixtures compile that a walk of src/ and tests/
        # for `.rs` never reads, and one outside the repository.
        for relative, body in FIXTURE_FILES.items():
            target = os.path.join(scratch if relative.startswith("../") else crate, relative.removeprefix("../"))
            os.makedirs(os.path.dirname(target), exist_ok=True)
            with open(target, "w") as extra:
                extra.write(body)
        os.symlink(os.path.join(crate, "linked_target"), os.path.join(crate, "tests", "linked"))
        messages, compiled, stderr = [], collections.defaultdict(set), ""
        for build in ([], ["--release"]):
            _, _, err, found, _, names = clippy(
                crate, ["--offline", "--keep-going"] + build, {"CARGO_TARGET_DIR": os.path.join(scratch, "target")}
            )
            messages += found
            stderr += err
            for source, read in names.items():
                compiled[source] |= read
        invocations, _, plain, problems, _, _, seen = classify(crate, messages, macro, None)
        problems += unrecorded_problems(seen, compiled)
        problems += compiled_problems(compiled) + cfg_problems(crate, compiled, also=(ROOT,))
        problems = judge(invocations, plain, problems, [("tests/accepted.rs", ("PATH",))], [])
        failures = 0
        for name, (_, reasons) in FIXTURES.items():
            file = f"tests/{name}.rs"
            mine = [why for f, _, why in problems if f in (file, *FIXTURE_OWNS.get(name, ()))]
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
        # A `.d` cut before its `# env-dep:` lines -- rustc writes them last
        # -- still holds the artifact's rule; what clippy's macro lint saw
        # shows what it lost. Cargo replays the fresh pass's reports.
        for directory, _, names in os.walk(os.path.join(scratch, "target")):
            for name in names:
                if name.endswith(".d"):
                    path = os.path.join(directory, name)
                    with open(path, "rb") as info:
                        kept = [line for line in info.read().split(b"\n") if not line.startswith(b"# env-dep:")]
                    with open(path, "wb") as info:
                        info.write(b"\n".join(kept))
        _, _, _, found, _, names = clippy(
            crate, ["--offline", "--keep-going"], {"CARGO_TARGET_DIR": os.path.join(scratch, "target")}
        )
        _, _, _, _, _, _, seen = classify(crate, found, macro, None)
        cut = [why for f, _, why in unrecorded_problems(seen, names) if f == "tests/option_env_of_a_key.rs"]
        failures += not any("dep-info without XFX_GATEWAY_KEY" in why for why in cut)
        print(f"{'refused' if cut else 'NOT REFUSED':9} dep-info cut before its env-dep lines"
              + "".join(f"\n            {why}" for why in cut))
        # The lexer alone, on text: what is blanked, what is kept, and that
        # every offset and newline stays where it was.
        lexed_wrong = []
        for what, text, kept in (
            ("nested comments", "a /* b /* c */ d */ e", "a                   e"),
            ("a line comment", "a // b ( c\nd", "a         \nd"),
            ("a byte string and a byte char", 'x(b")", b\'(\')', "x(    ,     )"),
            ("an escaped quote", 'f("a\\"b)", c)', "f(       , c)"),
            ("a unicode char", "f('\\u{1F600}', g)", "f(           , g)"),
            ("an escaped quote char", "f('\\'', g)", "f(    , g)"),
            ("a lifetime kept", "fn f<'a>(x: &'a u8) {}", "fn f<'a>(x: &'a u8) {}"),
            ("a raw string", 'f(r#"a")"#, g)', "f(        , g)"),
            ("newlines kept", '"a\nb" /* c\nd */ e', "  \n       \n     e"),
        ):
            got = code_of(text)
            if got != kept or len(got) != len(text) or [i for i, c in enumerate(got) if c == "\n"] != [
                i for i, c in enumerate(text) if c == "\n"
            ]:
                lexed_wrong.append(what)
        failures += bool(lexed_wrong)
        print(f"{'lexed' if not lexed_wrong else 'NOT LEXED for ' + str(lexed_wrong):9} comments, literals and lifetimes")
        # Where rustc's dep-info is, by artifact: an executable whose name
        # begins with "lib" keeps it, a library's file loses it, and an
        # uplifted executable is traced to its original, a hard link or a
        # copy.
        names_dir = os.path.join(scratch, "names", "deps")
        os.makedirs(names_dir)
        for name, body in (("library-0123456789abcdef", "a"), ("library-0123456789abcdef.d", ""),
                           ("libxfx-0123456789abcdef.rlib", ""), ("xfx-0123456789abcdef.d", ""),
                           ("xfx-fedcba9876543210", "the binary"), ("xfx-fedcba9876543210.d", ""),
                           ("xfx-0011223344556677", "another one"), ("xfx-0011223344556677.d", "")):
            with open(os.path.join(names_dir, name), "w") as made:
                made.write(body)
        linked = os.path.join(scratch, "names", "linked")
        os.makedirs(linked)
        os.link(os.path.join(names_dir, "xfx-fedcba9876543210"), os.path.join(linked, "xfx"))
        os.makedirs(os.path.join(linked, "deps"))
        os.link(os.path.join(names_dir, "xfx-fedcba9876543210"), os.path.join(linked, "deps", "xfx-fedcba9876543210"))
        open(os.path.join(linked, "deps", "xfx-fedcba9876543210.d"), "w").close()
        copied = os.path.join(scratch, "names", "xfx")
        shutil.copyfile(os.path.join(names_dir, "xfx-fedcba9876543210"), copied)
        # Two byte-identical originals of one copied file: no answer.
        twins = os.path.join(scratch, "twins")
        os.makedirs(os.path.join(twins, "deps"))
        for name in ("twin", "deps/twin-aaaaaaaaaaaaaaaa", "deps/twin-bbbbbbbbbbbbbbbb"):
            with open(os.path.join(twins, name), "w") as made:
                made.write("the same bytes")
        for name in ("deps/twin-aaaaaaaaaaaaaaaa.d", "deps/twin-bbbbbbbbbbbbbbbb.d"):
            open(os.path.join(twins, name), "w").close()
        resolved = {
            what: (dep_info(path) or (None,))[0]
            for what, path in (
                ("an executable named lib...", os.path.join(names_dir, "library-0123456789abcdef")),
                ("a library's .rlib", os.path.join(names_dir, "libxfx-0123456789abcdef.rlib")),
                ("an executable uplifted by a hard link", os.path.join(linked, "xfx")),
                ("an executable uplifted by a copy", copied),
                ("a copy with two byte-identical originals", os.path.join(twins, "twin")),
            )
        }
        expected = {
            "an executable named lib...": os.path.join(names_dir, "library-0123456789abcdef.d"),
            "a library's .rlib": os.path.join(names_dir, "xfx-0123456789abcdef.d"),
            "an executable uplifted by a hard link": os.path.join(linked, "deps", "xfx-fedcba9876543210.d"),
            "an executable uplifted by a copy": os.path.join(names_dir, "xfx-fedcba9876543210.d"),
            "a copy with two byte-identical originals": None,
        }
        wrong = [what for what in expected if resolved[what] != expected[what]]
        failures += bool(wrong)
        print(f"{'resolved' if not wrong else 'NOT RESOLVED for ' + str(wrong):9} dep-info by artifact kind")
        # The configuration: a .clippy.toml beside clippy.toml, and a
        # clippy.toml that lost a reader and a start.
        config = os.path.join(scratch, "config")
        os.makedirs(config)
        with open(os.path.join(ROOT, "clippy.toml")) as rules:
            text = rules.read()
        with open(os.path.join(config, "clippy.toml"), "w") as rules:
            rules.write("\n".join(line for line in text.splitlines()
                                  if '"std::env::var_os"' not in line and '"std::process::Command::new"' not in line))
        with open(os.path.join(config, ".clippy.toml"), "w") as rules:
            rules.write("")
        found = [why for _, _, why in config_problems(config)]
        # What cargo builds to run tests: an integration test binary with no
        # constructor, and a binary whose harness holds a case. Both must be
        # refused, in every build.
        built = os.path.join(scratch, "built")
        os.makedirs(os.path.join(built, "src"))
        os.makedirs(os.path.join(built, "tests"))
        with open(os.path.join(built, "Cargo.toml"), "w") as manifest:
            manifest.write('[package]\nname = "built"\nversion = "0.0.0"\nedition = "2021"\npublish = false\n')
        with open(os.path.join(built, "build.rs"), "w") as script:
            script.write('fn main() {\n    println!("cargo:rustc-cfg=hidden");\n}\n')
        with open(os.path.join(built, "src", "main.rs"), "w") as main:
            main.write("fn main() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn runs() {}\n}\n")
        with open(os.path.join(built, "tests", "plain.rs"), "w") as plain:
            # A compile-time read clippy never compiles: only the test
            # builds' dep-info can show it.
            plain.write('#[cfg(not(clippy))]\nconst _: &str = env!("CARGO_PKG_AUTHORS");\n\n#[test]\nfn runs() {}\n')
        built_target = {"CARGO_TARGET_DIR": os.path.join(scratch, "built-target")}
        artifacts, compiled, probed = artifact_problems(built, ("--offline",), built_target)
        artifacts += compiled_problems(compiled)
        # And again with every dep-info file gone: cargo calls the builds
        # fresh and rebuilds nothing, so nothing says what they read.
        for directory, _, names in os.walk(built_target["CARGO_TARGET_DIR"]):
            for name in names:
                if name.endswith(".d"):
                    os.remove(os.path.join(directory, name))
        _, compiled, _ = artifact_problems(built, ("--offline",), built_target)
        artifacts += [(f, line, "after the dep-info is gone: " + why) for f, line, why in compiled_problems(compiled)]
        # Then each `.d` empty, then each cut inside its first line: neither
        # holds the artifact's own rule, so neither is evidence.
        for label, cut in (
            ("empty", lambda text: b""),
            ("truncated", lambda text: text[: len(text.split(b"\n")[0]) // 2]),
            # rustc's first rule is the `.d` file's own; cut after it, what
            # is left is a well-formed rule for some other file.
            ("cut after its first rule", lambda text: text.split(b"\n")[0] + b"\n"),
        ):
            # Sources newer than the last build, so the builds run once more
            # and rustc writes every `.d` afresh, then each one cut.
            # Not later than now: a source dated in the future would keep
            # every next build stale, and rustc would write the `.d` again.
            now = time.time()
            for source in ("build.rs", "src/main.rs", "tests/plain.rs"):
                os.utime(os.path.join(built, source), (now, now))
            artifact_problems(built, ("--offline",), built_target)
            for directory, _, names in os.walk(built_target["CARGO_TARGET_DIR"]):
                for name in names:
                    if name.endswith(".d"):
                        path = os.path.join(directory, name)
                        with open(path, "rb") as info:
                            text = info.read()
                        with open(path, "wb") as info:
                            info.write(cut(text))
            _, compiled, _ = artifact_problems(built, ("--offline",), built_target)
            artifacts += [(f, line, f"with every dep-info {label}: " + why) for f, line, why in compiled_problems(compiled)]
        artifacts += build_script_problems(built, compiled)
        refusals = [why for _, _, why in artifacts]
        wanted_built = [
            ("tests/plain.rs", "without making a clean copy", len(TEST_BUILDS)),
            ("src/main.rs", "that holds cases", len(TEST_BUILDS)),
            ("tests/plain.rs", "compiled with CARGO_PKG_AUTHORS", 1),
            ("tests/plain.rs", "after the dep-info is gone: built with no dep-info", 1),
            ("src/main.rs", "after the dep-info is gone: built with no dep-info", 1),
            ("tests/plain.rs", "with every dep-info empty: built with no dep-info", 1),
            ("tests/plain.rs", "with every dep-info truncated: built with no dep-info", 1),
            ("tests/plain.rs", "with every dep-info cut after its first rule: built with no dep-info", 1),
            ("build.rs", "sets a cfg (rustc-cfg)", 1),
        ]
        missing_built = [
            (file, reason) for file, reason, count in wanted_built
            if sum(1 for f, _, why in artifacts if f == file and reason in why) != count
        ]
        failures += bool(missing_built) or probed != len(TEST_BUILDS)
        print(f"{'refused' if not missing_built else 'NOT REFUSED for ' + str(missing_built):9} test binaries cargo built"
              + "".join(f"\n            {why}" for why in refusals))
        wanted = ["a .clippy.toml", "no longer holds std::env::var_os", "no longer holds std::process::Command::new"]
        missing = [reason for reason in wanted if not any(reason in why for why in found)]
        failures += bool(missing)
        print(f"{'refused' if not missing else 'NOT REFUSED for ' + str(missing):9} clippy configuration"
              + "".join(f"\n            {why}" for why in found))
        print(f"check-env-reads --self-test: {len(FIXTURES) + 5} fixtures, {failures} not as expected")
        return 1 if failures else 0
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        sys.exit(self_test())
    if sys.argv[1:]:
        sys.exit("usage: check-env-reads.py [--self-test]")
    sys.exit(check_tree())
