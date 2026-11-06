"""Run each mutant on the throwaway clone (never the worktree) and record
which named tests fail. usage: mutants.py <clone> <log>"""
import subprocess
import sys

clone, log = sys.argv[1], sys.argv[2]
assert "/scratchpad/" in clone, clone

MUTANTS = [
    (
        "M1 drop .mode(0o700) from the one-level make",
        "src/profile_home.rs",
        "fs::DirBuilder::new().mode(DIR_MODE).create(path)",
        "fs::DirBuilder::new().create(path)",
    ),
    (
        "M2 bypass the helper at /skills create",
        "src/skills/command.rs",
        "super::install::ensure_managed(managed, &dir)",
        "std::fs::create_dir_all(&dir)",
    ),
    (
        "M3 bypass the helper at the settings document writer",
        "src/provider/profile.rs",
        "    crate::profile_home::ensure(dir, dir)?;\n    replace_private_file(dir, path, document)",
        "    std::fs::create_dir_all(dir)?;\n    replace_private_file(dir, path, document)",
    ),
    (
        "M4 accept an existing home without the check",
        "src/profile_home.rs",
        "    match problem(profile)? {\n        None => Ok(()),",
        "    match None::<Problem> {\n        None => Ok(()),",
    ),
    (
        "M5 drop .mode(0o700) from the ancestors' make",
        "src/profile_home.rs",
        "        .recursive(true)\n        .mode(DIR_MODE)\n",
        "        .recursive(true)\n",
    ),
]

env = {
    "PATH": __import__("os").environ["PATH"],
    "HOME": __import__("os").environ["HOME"],
    "TMPDIR": "<tmp>",
}
with open(log, "w") as out:
    for name, path, old, new in MUTANTS:
        full = f"{clone}/{path}"
        text = open(full).read()
        assert text.count(old) == 1, (name, text.count(old))
        open(full, "w").write(text.replace(old, new))
        try:
            run = subprocess.run(
                ["timeout", "900", "cargo", "test", "--locked", "--lib", "profile_home"],
                cwd=clone,
                env=env,
                capture_output=True,
                text=True,
            )
        finally:
            open(full, "w").write(text)
        failed = [
            line[5:].removesuffix(" ... FAILED")
            for line in run.stdout.splitlines()
            if line.startswith("test ") and line.endswith("FAILED")
        ]
        summary = [l for l in run.stdout.splitlines() if l.startswith("test result")]
        out.write(f"## {name}\nexit={run.returncode} {summary}\n")
        for test in failed:
            out.write(f"  FAILED {test}\n")
        if run.returncode != 0 and not failed:
            out.write(run.stderr[-3000:] + "\n")
        out.flush()
    status = subprocess.run(["git", "-C", clone, "status", "--short"], capture_output=True, text=True)
    out.write(f"## clone clean after mutants: {status.stdout.strip() == ''}\n")
