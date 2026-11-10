"""This unit's mutants, each on a throwaway clone (`git clone --no-local` of
the head into <clone>, never the worktree): one edit of the head's source,
then the tests that must catch it, with a bounded run. A mutant is KILLED
when at least one of its tests fails, SURVIVED when all pass. The clone is
reset between mutants. (PROFILE-REFUSE-AT-LAUNCH's runner, its mutants
replaced by this unit's.)

    M1  typed keys: a later key's error never refuses (the class is dropped)
    M2  a key twice: the duplicate check always passes
    M3  provider: the lookup of a connection name no longer stops
    M4  models: the routing check no longer reads `models`
    M5  status no longer stops on a failed load
    M6  the context limits' error refuses (fx's spares the launch)
    M7  the stop names the routing keys' error, not the layer's first
    M8  a `statusLine` that is not an object refuses in the entry too
    -- round 2:
    M9  any `providers` object exempts a name again (MUST 1)
    M10 a `providers` that is not an object no longer stops
    M11 33 connections pass (fx's limit is 32)
    M12 a layer may name 36 model preferences (fx's limit is 35)
    M13 the two layers merged may name up to 70
    M14 the `model`'s Gateway no longer counts in a whole parse
    M15 doctor no longer says the load failed
    M16 the skill authorities' error refuses (fx's spares the launch)
    M17 the retired key's error refuses (fx's spares the launch)
    M18 `models` read in reverse, not in the order the file wrote it

usage: python3 -I mutants.py <head repo> <clone dir>
"""

import os
import shutil
import subprocess
import sys

sys.dont_write_bytecode = True

CLI = ("--test", "cli")
SHELL = ("--test", "interactive")
TUI = ("--test", "tui")
LIB = ("--lib",)
TABLE = (LIB, "config::tests::a_launch_refuses_what_fxs_parse_of_the_profile_refuses")
MODELS = (LIB, "config::tests::a_launch_refuses_the_models_fx_refuses_and_no_other")
ASK = (CLI, "ask_refuses_a_profile_fx_refuses")
STARTS = (CLI, "ask_goes_on_with_a_profile_fx_starts_with")
STATUS = (CLI, "status_stops_where_fxs_load_fails")

MUTANTS = [
    ("M1", "src/config.rs",
     "        .map(|(_, _, refuses)| *refuses)",
     "        .map(|_| false)",
     [TABLE, ASK, (SHELL, "the_shell_refuses_a_profile_fx_refuses"), (TUI, "a_profile_fx_refuses_is_refused")]),
    ("M2", "src/config.rs",
     "        Ok(Value::Object(object)) if serde_json::from_str::<KeysOnce>(&text).is_ok() => {",
     "        Ok(Value::Object(object)) if serde_json::from_str::<KeysOnce>(&text).is_ok() || true => {",
     [TABLE, (LIB, "config::tests::a_key_twice_is_a_file_that_does_not_parse"), ASK]),
    ("M3", "src/config.rs", "    if unknown {", "    if unknown && false {", [TABLE, ASK, STATUS]),
    ("M4", "src/config.rs",
     "    let models = match layer.get(MODELS_KEY) {",
     "    let models = match None::<&Value> {",
     [TABLE, ASK, STATUS]),
    ("M5", "src/app.rs",
     "            if let Some(refusal) = config.launch_refusal(crate::config::Launch::Status) {",
     "            if let Some(refusal) = None::<&str> {",
     [STATUS]),
    ("M6", "src/config.rs",
     '        ("context_limits", &fx_context_limits, false),',
     '        ("context_limits", &fx_context_limits, true),',
     [TABLE, STARTS]),
    ("M7", "src/config.rs",
     "    fx_model_error(layer).or(fx_routing(layer, true).err())",
     "    fx_routing(layer, true).err()",
     [TABLE]),
    ("M8", "src/config.rs",
     "    let status_line = |v: &Value| v.as_object().map_or(!global, |o| bools(o, items));",
     "    let status_line = |v: &Value| v.as_object().map_or(false, |o| bools(o, items));",
     [TABLE, STARTS]),
    ("M9", "src/config.rs",
     "            && !defined.is_some_and(|defined| defined.contains_key(name))",
     "            && defined.is_none()",
     [TABLE, ASK, STATUS]),
    ("M10", "src/config.rs", '        _ => Some("InvalidObject"),', "        _ => None,", [TABLE, ASK, STATUS]),
    ("M11", "src/config.rs", "(providers.len() > 32)", "(providers.len() > 33)", [TABLE]),
    ("M12", "src/config.rs",
     "            Value::String(_) if named.len() == FX_MAX_PREFERENCES => {",
     "            Value::String(_) if named.len() == FX_MAX_PREFERENCES + 1 => {",
     [TABLE]),
    ("M13", "src/config.rs",
     "        if preferences.len() > FX_MAX_PREFERENCES {",
     "        if preferences.len() > FX_MAX_PREFERENCES * 2 {",
     [TABLE]),
    ("M14", "src/config.rs",
     "        .filter(|(key, _)| whole && layer.contains_key(*key))",
     "        .filter(|(key, _)| !whole && layer.contains_key(*key))",
     [TABLE]),
    ("M15", "src/app.rs",
     '        checks.insert(1, fail("config", "failed to load config"));',
     "",
     [STATUS]),
    ("M16", "src/config.rs",
     "        (SKILL_SYMLINK_AUTHORITIES_KEY, &authorities, false),",
     "        (SKILL_SYMLINK_AUTHORITIES_KEY, &authorities, true),",
     [TABLE, STARTS]),
    ("M17", "src/config.rs", "        return Some(false);", "        return Some(true);", [TABLE, MODELS, STARTS]),
    ("M18", "src/config.rs",
     "    for (name, model) in models {",
     "    for (name, model) in models.iter().rev() {",
     [TABLE]),
]


def run(cmd, cwd, timeout):
    try:
        p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)
        return p.returncode, p.stdout + p.stderr
    except subprocess.TimeoutExpired:
        return "timeout", ""


def edit(clone, path, old, new):
    full = os.path.join(clone, path)
    text = open(full, encoding="utf-8").read()
    if text.count(old) != 1:
        raise SystemExit(f"{path}: the mutant's text is not there exactly once")
    open(full, "w", encoding="utf-8").write(text.replace(old, new, 1))


def main():
    head, clone = sys.argv[1], sys.argv[2]
    if os.path.realpath(clone).startswith(os.path.realpath(head)):
        raise SystemExit("the clone must not be inside the worktree")
    shutil.rmtree(clone, ignore_errors=True)
    subprocess.run(["git", "clone", "-q", "--no-local", head, clone], check=True)
    rev = subprocess.run(["git", "-C", clone, "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
    print(f"clone of {rev}")
    env_note = "TMPDIR is the caller's; each test run bounded at 900 s"
    print(env_note)
    for mutant in MUTANTS:
        name, path, old, new, tests = mutant[:5]
        subprocess.run(["git", "-C", clone, "checkout", "-q", "--", "."], check=True)
        edit(clone, path, old, new)
        if len(mutant) > 5:
            edit(clone, *mutant[5])
        failed = []
        for target, test in tests:
            code, out = run(["cargo", "test", "--locked", "-q", *target, "--", test], clone, 900)
            if code != 0:
                tail = [line for line in out.splitlines() if "panicked at" in line or line.startswith("error")]
                failed.append(f"{target[-1]}::{test} ({code}; {tail[0].split(' at ')[-1] if tail else 'no panic line'})")
        verdict = "KILLED" if failed else "SURVIVED"
        print(f"{name} {verdict}: " + ("; ".join(failed) if failed else "every test passed"))
    subprocess.run(["git", "-C", clone, "checkout", "-q", "--", "."], check=True)


if __name__ == "__main__":
    main()
