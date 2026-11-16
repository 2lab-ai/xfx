#!/usr/bin/env python3
"""RED and GREEN for the inherited marker (round 4, gpt6 isolation MUST).

    TMPDIR=<short> red-marker.py <clone> <rev>...

For each <rev>, on a throwaway clone: `process_env!`'s build arms are made to
count their reads into `$TMPDIR/xfx-marker-reads.log` (one line per read: the
reader and the variable's name, never a value), tests/permissions.rs is
built, and its binary is started as an old copy's environment or a
developer's shell that kept the marker would start it -- `XFX_TEST_ISOLATED_COPY`
already set, `HOME` a sentinel home holding a `.gitconfig` with a hook and a
token-carrying `insteadOf` -- asked for one case that plans git in its own
process. Each run prints its exit status, libtest's result line, whether the
binary refused to run any case, and how many reads the case made: a read
there is a read under that home.

Two markers: round 3's value, `1`, and one of the shape round 4's parent
makes, with the sentinel home named after it.
"""
import os
import pathlib
import shutil
import subprocess
import sys

CASE = "auto_does_not_run_git_in_a_directory_whose_configuration_defines_a_hook"
WELL_FORMED = "0123456789abcdef0123456789abcdef"

RECORD = (
    '{{ {{ use ::std::io::Write as _; '
    'if let Some(dir) = ::std::env::var_os("TMPDIR") {{ '
    'if let Ok(mut log) = ::std::fs::OpenOptions::new().create(true).append(true)'
    '.open(::std::path::Path::new(&dir).join("xfx-marker-reads.log")) {{ '
    'let line = format!("{{}}\\t{{}}\\n", $who, {name}); '
    'let _ = log.write_all(line.as_bytes()); }} }} }} {read} }}'
)


def instrument(clone):
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


def sentinel_home(root, name):
    home = root / name
    home.mkdir(mode=0o700, parents=True)
    (home / ".gitconfig").write_text(
        '[hook "sentinel"]\n\tcommand = true\n'
        '[url "https://x-access-token:xfx-sentinel-git-token@sentinel.invalid/"]\n'
        "\tinsteadOf = https://github.com/\n"
    )
    return home


def main():
    clone = pathlib.Path(sys.argv[1]).resolve()
    assert (clone / ".git").is_dir(), "a clone, not a worktree"
    tmp = pathlib.Path(os.environ["TMPDIR"])
    log = tmp / "xfx-marker-reads.log"
    for rev in sys.argv[2:]:
        assert subprocess.run(["git", "checkout", "-q", "--force", rev], cwd=clone).returncode == 0
        instrument(clone)
        build = subprocess.run(["cargo", "test", "--locked", "--test", "permissions", "--no-run"],
                               cwd=clone, text=True, capture_output=True)
        assert build.returncode == 0, build.stderr[-2000:]
        binary = next(clone / line.split("(")[1].rstrip(")") for line in build.stderr.splitlines()
                      if "Executable tests/permissions.rs" in line)
        print(f"# at {rev}: tests/permissions.rs, --exact {CASE}")
        for marker in ("1", WELL_FORMED):
            root = tmp / "red-marker"
            shutil.rmtree(root, ignore_errors=True)
            home = sentinel_home(root, f"xfx-test-home-{marker}")
            if log.exists():
                log.unlink()
            env = {"PATH": os.environ["PATH"], "TMPDIR": str(tmp), "HOME": str(home),
                   "XFX_TEST_ISOLATED_COPY": marker}
            run = subprocess.run([str(binary), "--exact", CASE, "--test-threads=1"], cwd=clone,
                                 env=env, text=True, capture_output=True, timeout=300)
            output = run.stdout + run.stderr
            result = [line for line in run.stdout.splitlines() if line.startswith("test result")]
            refused = [line for line in run.stderr.splitlines() if "no case runs" in line]
            reads = log.read_text().splitlines() if log.exists() else []
            leaked = [value for value in ("xfx-sentinel-git-token", "sentinel.invalid") if value in output]
            readers = sorted(set(reads))
            print(f"marker {'round 3 (1)' if marker == '1' else 'well-formed'}: exit {run.returncode}; "
                  f"{result[-1] if result else 'no test result'}; "
                  f"refused: {refused[0].split(': ', 1)[1] if refused else 'no'}; "
                  f"reads under the sentinel home: {len(reads)} {readers}; sentinel values in output: {len(leaked)}")
            shutil.rmtree(root, ignore_errors=True)
        subprocess.run(["git", "checkout", "-q", "--force", rev], cwd=clone)
        print()


if __name__ == "__main__":
    main()
