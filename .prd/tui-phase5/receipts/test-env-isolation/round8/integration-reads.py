#!/usr/bin/env python3
"""Every read of the process environment an integration test binary makes,
and whether the process that made it is the one the developer started or
the clean copy it becomes (round 8; round 4's script, its log moved).

    TMPDIR=<short> integration-reads.py <clone> <rev> [--log-under-tmpdir] [suite...]

Round 4's instrumentation wrote its log under each reading process's own
TMPDIR, so a case that hands a child a TMPDIR of its own and then looks at
what is in it -- tests/isolation.rs's relative-TMPDIR parent case -- found
the log there and failed. The log now goes to one absolute path fixed when
the clone is built (`env!` of XFX_ENV_READS_LOG, in the instrumented code
only); `--log-under-tmpdir` keeps round 4's place, to show the difference.
Each failing case is named, with the first line of its panic, a scratch
path cut to <tmp>.

On a throwaway clone at <rev>, each build arm of `process_env!` (src/lib.rs)
and, where the file exists, each arm of `env_exception!`
(src/env_exception.rs) is made to append one line -- the reading binary's
file name, "copy" or "parent" by the marker, its thread's name (libtest names
it after the case), `$who` or the macro, and the variable's name, never a
value -- to that log, then reads as before. Every
integration suite then runs under sentinel-run.sh. Lines whose binary is not
`xfx` are reads in a test binary; `xfx` lines are the binary the suites
start, whose environment `clear_env` built.
"""
import os
import pathlib
import re
import subprocess
import sys
from collections import Counter, defaultdict

HERE = pathlib.Path(__file__).resolve().parent
SENTINEL_RUN = HERE.parent / "sentinel-run.sh"

UNDER_TMPDIR = "--log-under-tmpdir" in sys.argv
LOG_PLACE = (
    'if let Some(dir) = ::std::env::var_os("TMPDIR") {{ '
    'if let Ok(mut log) = ::std::fs::OpenOptions::new().create(true).append(true)'
    '.open(::std::path::Path::new(&dir).join("xfx-env-reads.log")) {{ '
    if UNDER_TMPDIR else
    'if let Some(()) = Some(()) {{ '
    'if let Ok(mut log) = ::std::fs::OpenOptions::new().create(true).append(true)'
    '.open(env!("XFX_ENV_READS_LOG")) {{ '
)
RECORD = (
    '{{ {{ use ::std::io::Write as _; ' + LOG_PLACE +
    'let exe = ::std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default(); '
    'let thread = ::std::thread::current().name().unwrap_or("?").to_string(); '
    'let copy = if ::std::env::var_os("XFX_TEST_ISOLATED_COPY").is_some() {{ "copy" }} else {{ "parent" }}; '
    'let line = format!("{{}}\\t{{}}\\t{{}}\\t{{}}\\t{{}}\\n", exe, copy, thread, $who, {name}); '
    'let _ = log.write_all(line.as_bytes()); }} }} }} {read} }}'
)


def main():
    clone = pathlib.Path(sys.argv[1]).resolve()
    rev = sys.argv[2]
    assert (clone / ".git").is_dir(), "a clone, not a worktree"
    assert subprocess.run(["git", "checkout", "-q", "--force", rev], cwd=clone).returncode == 0
    lib = clone / "src/lib.rs"
    text = lib.read_text()
    arms = {
        "() => ::std::env::var($name),": "() => " + RECORD.format(name="::std::ffi::OsStr::new(&$name).to_string_lossy()", read="::std::env::var($name)") + ",",
        "() => ::std::env::var_os($name),": "() => " + RECORD.format(name="::std::ffi::OsStr::new(&$name).to_string_lossy()", read="::std::env::var_os($name)") + ",",
        "() => ::std::env::var(name).ok(),": "() => " + RECORD.format(name="name", read="::std::env::var(name).ok()") + ",",
    }
    for old, new in arms.items():
        assert old in text, old
        text = text.replace(old, new)
    lib.write_text(text)
    exception = clone / "src/env_exception.rs"
    if exception.exists():
        text = exception.read_text()
        arms = {
            "() => ::std::env::var($name),": "() => " + RECORD.format(name="$name", read="::std::env::var($name)").replace("$who", '"env_exception!"') + ",",
            "() => ::std::env::var_os($name),": "() => " + RECORD.format(name="$name", read="::std::env::var_os($name)").replace("$who", '"env_exception!"') + ",",
            "() => ::std::env::vars_os().map(|(name, _)| name),": "() => " + RECORD.format(name='"names()"', read="::std::env::vars_os().map(|(name, _)| name)").replace("$who", '"env_exception!"') + ",",
        }
        for old, new in arms.items():
            if old in text:
                text = text.replace(old, new)
        list_arm = "() => [$($name),+]"
        assert list_arm in text, list_arm
        text = text.replace(list_arm, "() => (" + RECORD.format(name='[$($name),+].join(",")', read="[$($name),+]").replace("$who", '"env_exception!"') + ")")
        exception.write_text(text)
    log = pathlib.Path(os.environ["TMPDIR"]) / "xfx-env-reads.log"
    if log.exists():
        log.unlink()
    environment = dict(os.environ, XFX_ENV_READS_LOG=str(log))
    build = subprocess.run(["cargo", "test", "--locked", "--tests", "--no-run"], cwd=clone,
                           text=True, capture_output=True, env=environment)
    assert build.returncode == 0, build.stderr[-3000:]
    only = [arg for arg in sys.argv[3:] if not arg.startswith("--")]
    suites = sorted(p.stem for p in (clone / "tests").glob("*.rs") if not only or p.stem in only)
    binaries = {}
    for line in build.stderr.splitlines():
        m = re.search(r"Executable tests/(\w+)\.rs \((.*)\)", line)
        if m:
            binaries[m.group(1)] = clone / m.group(2)
    results, failures = {}, {}
    roots = "|".join([r"/(private/)?tmp/claude-[0-9]+", "/var/" + "folders/", "/" + "Users/"])
    for suite in suites:
        out = subprocess.run([str(SENTINEL_RUN), str(binaries[suite]), "--test-threads=4"],
                             cwd=clone, text=True, capture_output=True)
        summary = [l for l in out.stdout.splitlines() if l.startswith("test result")]
        results[suite] = summary[-1] if summary else "no result"
        # Each failing case, and the first line of its panic.
        lines = out.stdout.splitlines()
        for at, line in enumerate(lines):
            found = re.match(r"^---- (\S+) stdout ----$", line)
            if not found:
                continue
            said = next((l for l in lines[at + 1 :] if "panicked at" in l or l.strip().startswith(("assertion", "called "))), "")
            said = next((l for l in lines[at + 1 : at + 12] if l.strip() and "panicked at" not in l
                         and not l.startswith("note:")), said)
            failures.setdefault(suite, []).append((found.group(1), re.sub(rf"(?:{roots})[^\s\"'`]*", "<tmp>", said.strip())[:220]))
    subprocess.run(["git", "checkout", "-q", "--force", rev], cwd=clone)
    lines = log.read_text(errors="replace").splitlines() if log.exists() else []
    rows = [line.split("\t") for line in lines if line.count("\t") == 4]
    print(f"# log lines {len(lines)}, well-formed {len(rows)} (one write per record)")
    print(f"# at {rev}; every integration suite under sentinel-run.sh, build arms recording; "
          f"log {'under each process TMPDIR (round 4)' if UNDER_TMPDIR else 'at one absolute path fixed at build'}")
    for suite in suites:
        print(f"suite {suite}: {results[suite]}")
        for case, said in failures.get(suite, []):
            print(f"  failed: {case} -- {said}")
    copies = [r for r in rows if r[0] != "xfx" and r[1] == "copy"]
    in_process = [r[:1] + r[2:] for r in rows if r[0] != "xfx" and r[1] == "parent"]
    print(f"\n# reads recorded: {len(rows)} ({len(rows) - len(in_process) - len(copies)} by the xfx binary the suites start, {len(copies)} in a clean copy of a test binary, {len(in_process)} in the test process the developer started)")
    print(f"# of those in the developer's process, through process_env!: {sum(1 for r in in_process if r[2] != 'env_exception!')}")
    by_suite = defaultdict(set)
    readers = Counter()
    for exe, thread, who, name in in_process:
        suite = re.sub(r"-[0-9a-f]+$", "", exe)
        by_suite[suite].add(thread)
        readers[(suite, who, name)] += 1
    print("\n# in a test's own process, by suite, reader and variable: reads")
    for (suite, who, name), count in sorted(readers.items()):
        print(f"{suite}\t{who}\t{name}\t{count}")
    print("\n# in a test's own process, the cases (threads) that read, by suite")
    for suite in sorted(by_suite):
        print(f"## {suite}: {len(by_suite[suite])}")
        for thread in sorted(by_suite[suite]):
            print(f"  {thread}")


if __name__ == "__main__":
    main()
