"""Run each mutant on the throwaway clone (never the worktree) and record
which named tests fail. usage: mutants.py <clone> <log>"""
import os
import subprocess
import sys

clone, log = sys.argv[1], sys.argv[2]
assert "/scratchpad/" in clone, clone

LIB = ["cargo", "test", "--locked", "--lib"]
MUTANTS = [
    (
        "M1 drop .mode(0o700) from the one-level make",
        "src/profile_home.rs",
        "fs::DirBuilder::new().mode(DIR_MODE).create(path)",
        "fs::DirBuilder::new().create(path)",
        ["profile_home"],
    ),
    (
        "M2 bypass the helper at /skills create",
        "src/skills/command.rs",
        "super::install::ensure_managed(managed, &dir)",
        "std::fs::create_dir_all(&dir)",
        ["profile_home"],
    ),
    (
        "M3 bypass the helper at the settings document writer",
        "src/provider/profile.rs",
        "    crate::profile_home::ensure(dir, dir)?;\n    replace_private_file(dir, path, document)",
        "    std::fs::create_dir_all(dir)?;\n    replace_private_file(dir, path, document)",
        ["profile_home"],
    ),
    (
        "M4 accept an existing home without the check",
        "src/profile_home.rs",
        "    refuse_unfit(profile)\n}",
        "    Ok(())\n}",
        ["profile_home"],
    ),
    (
        "M5 make a missing home again (round 1's make_all)",
        "src/profile_home.rs",
        "                if !home.as_os_str().is_empty() && !home.is_dir() {\n                    return Err(",
        "                if !home.as_os_str().is_empty() && !home.is_dir() {\n                    use std::os::unix::fs::DirBuilderExt;\n                    fs::DirBuilder::new().recursive(true).mode(DIR_MODE).create(home)?;\n                }\n                if false {\n                    return Err(",
        ["profile_home"],
    ),
    (
        "M6 follow a link at a private level (treat it as content)",
        "src/profile_home.rs",
        "                Kind::Private => refuse_unfit(&level)?,",
        "                Kind::Private if level.is_dir() => {}\n                Kind::Private => return Err(err),",
        ["profile_home"],
    ),
    (
        "M7 /allowlist takes settings.lock before the home is checked",
        "src/provider/profile.rs",
        "    crate::profile_home::ensure(dir, dir)?;\n    let _across_processes = SettingsLock::acquire(dir, SETTINGS_LOCK_DEADLINE)?;\n    // Read again under the lock",
        "    let _across_processes = SettingsLock::acquire(dir, SETTINGS_LOCK_DEADLINE)?;\n    // Read again under the lock",
        ["profile_home"],
    ),
    (
        "M8 the store makes a missing parent again (base's create_dir_all)",
        "src/session/store.rs",
        "        Err(err) if err.kind() == io::ErrorKind::NotFound => {}\n        Err(err) => return Err(io_error(path, err)),\n    }\n    match create_private_dir(path) {",
        "        Err(err) if err.kind() == io::ErrorKind::NotFound => {}\n        Err(err) => return Err(io_error(path, err)),\n    }\n    if let Some(parent) = path.parent() {\n        if !parent.is_dir() {\n            fs::create_dir_all(parent).map_err(|err| io_error(parent, err))?;\n        }\n    }\n    match create_private_dir(path) {",
        ["session::store::fork", "profile_home"],
    ),
    (
        "M9 the source cut stops at the first test module (round 1's)",
        "src/profile_home/tests/scan.rs",
        "    let clean = strip_comments_and_literals(text);\n    cut_test_items(&clean)",
        "    let clean = strip_comments_and_literals(text);\n    let cut = clean.find(\"#[cfg(test)]\\nmod tests\").unwrap_or(clean.len());\n    (clean[..cut].to_string(), Vec::new())",
        ["profile_home"],
    ),
    (
        "M10 the bounded wait does not kill its child",
        "src/process.rs",
        "        if started.elapsed() >= limit {\n            // This test's own child",
        "        if false && started.elapsed() >= limit {\n            // This test's own child",
        ["process::tests::a_child_past_its_limit_is_killed_and_reaped"],
    ),
]

env = {
    "PATH": os.environ["PATH"],
    "HOME": os.environ["HOME"],
    "TMPDIR": "<tmp>",
}
with open(log, "w") as out:
    only = sys.argv[3:] 
    for name, path, old, new, filters in MUTANTS:
        if only and not any(name.startswith(o) for o in only):
            continue
        full = f"{clone}/{path}"
        text = open(full).read()
        assert text.count(old) == 1, (name, text.count(old))
        open(full, "w").write(text.replace(old, new))
        failed, summary, stderr, code = [], [], "", 0
        try:
            for filt in filters:
                run = subprocess.run(
                    ["timeout", "900"] + LIB + [filt],
                    cwd=clone, env=env, capture_output=True, text=True,
                )
                code = code or run.returncode
                failed += [
                    line[5:].removesuffix(" ... FAILED")
                    for line in run.stdout.splitlines()
                    if line.startswith("test ") and line.endswith("FAILED")
                ]
                summary += [l for l in run.stdout.splitlines() if l.startswith("test result")]
                stderr += run.stderr[-3000:]
        finally:
            open(full, "w").write(text)
        out.write(f"## {name}\nexit={code} {summary}\n")
        for test in failed:
            out.write(f"  FAILED {test}\n")
        if code != 0 and not failed:
            out.write(stderr + "\n")
        out.flush()
    status = subprocess.run(["git", "-C", clone, "status", "--short"], capture_output=True, text=True)
    out.write(f"## clone clean after mutants: {status.stdout.strip() == ''}\n")
