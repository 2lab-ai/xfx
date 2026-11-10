"""This unit's mutants, each on a throwaway clone (`git clone --no-local` of
the head into <clone>, never the worktree): one edit of the head's source,
then the tests that must catch it, with a bounded run. A mutant is KILLED
when at least one of its tests fails, SURVIVED when all pass. The clone is
reset between mutants. (PROFILE-REFUSE-TYPED-KEYS's runner, its mutants
replaced by this unit's.)

One per surface:
    M1  the settings key (profile and entry) reads the exact wire parse again
    M2  XFX_PERMISSION_MODE reads the exact wire parse again
    M3  XFX_PERMISSION_MODE is trimmed again before the parse
    M4  the launch refusal's check reads the exact wire parse
    M5  `--full-access` no longer sets full access (round 2: two flags now)
    M6  the `[status]` line says the wire word again
    M7  the `[doctor]` line says the wire word again
    M8  doctor's startup check says the wire word again
    M14 `/permissions` no longer trims its line (the mode's parse does not);
        round 2: caught by its own test, not the bare-blank assertion
    -- round 2:
    M15 a blank XFX_PERMISSION_MODE is read (and complained of) again
    M16 `--full-access`'s help line is not fx's
    M17 doctor's own `permissions` check says the wire word again
    M18 `--auto` and `--full-access` together are taken
One per alias, in the parse every surface shares:
    M9  `full-access` is not the third mode
    M10 `full access` is not the third mode
    M11 `yolo` is not the third mode
    M12 the words are compared with their case
    M13 the word is trimmed before it is compared

usage: python3 -I mutants.py <head repo> <clone dir>
"""

import os
import shutil
import subprocess
import sys

sys.dont_write_bytecode = True

LIB = ("--lib",)
CLI = ("--test", "cli")
GATEWAY = ("--test", "gateway")
WORDS_UNIT = (LIB, "config::tests::a_typed_mode_reads_upstreams_words_in_any_case")
SURFACES_UNIT = (LIB, "config::tests::a_mode_is_read_with_fxs_parse_wherever_a_person_writes_one")
TABLE = (LIB, "config::tests::a_launch_refuses_what_fxs_parse_of_the_profile_refuses")
FLAGS_UNIT = (LIB, "cli::tests::the_permission_flags_select_a_mode_and_cannot_be_combined")
SLASH_TRIM = (LIB, "permission::slash::tests::the_line_is_trimmed_before_the_modes_parse_which_trims_nothing")
SURFACES = (CLI, "a_mode_in_any_of_fxs_spellings_is_the_mode_that_runs")
SPOKEN = (CLI, "full_access_is_said_in_fxs_word_and_recorded_in_the_wire_word")
FLAG = (CLI, "ask_takes_fxs_two_spellings_of_full_access_exactly")
HELP = (GATEWAY, "ask_help_advertises_only_the_implemented_flags")
THIRD = '        } else if is("full-access") || is("full access") || is("yolo") {'

MUTANTS = [
    ("M1", "src/config.rs",
     "            match value.as_str().and_then(PermissionMode::parse_choice) {",
     "            match value.as_str().and_then(PermissionMode::parse) {",
     [SURFACES_UNIT, SURFACES]),
    ("M2", "src/config.rs",
     "        match PermissionMode::parse_choice(raw) {",
     "        match PermissionMode::parse(raw) {",
     [SURFACES_UNIT, SURFACES]),
    ("M3", "src/config.rs",
     "        .filter(|raw| !raw.trim().is_empty())",
     "        .map(str::trim)\n        .filter(|raw| !raw.is_empty())",
     [SURFACES_UNIT, SURFACES]),
    ("M4", "src/config.rs",
     "v.as_str().and_then(PermissionMode::parse_choice).is_some()",
     "v.as_str().and_then(PermissionMode::parse).is_some()",
     [TABLE]),
    ("M5", "src/cli.rs",
     "                let mode = if yolo || full_access {",
     "                let mode = if yolo {",
     [FLAGS_UNIT, FLAG]),
    ("M6", "src/output.rs",
     '        line("permission_mode", &display_mode(&self.permission_mode));',
     '        line("permission_mode", &self.permission_mode);',
     [SPOKEN]),
    ("M7", "src/output.rs",
     "            display_mode(&self.permission_mode)\n",
     "            self.permission_mode\n",
     [SPOKEN]),
    ("M8", "src/app.rs",
     "            config.permission_mode.display_label(),",
     "            config.permission_mode.label(),",
     [SPOKEN]),
    ("M9", "src/config.rs", THIRD,
     '        } else if is("full access") || is("yolo") {',
     [WORDS_UNIT, SURFACES]),
    ("M10", "src/config.rs", THIRD,
     '        } else if is("full-access") || is("yolo") {',
     [WORDS_UNIT, SURFACES]),
    ("M11", "src/config.rs", THIRD,
     '        } else if is("full-access") || is("full access") {',
     [WORDS_UNIT, SURFACES]),
    ("M12", "src/config.rs",
     "        let is = |word: &str| raw.eq_ignore_ascii_case(word);",
     "        let is = |word: &str| raw == word;",
     [WORDS_UNIT, SURFACES]),
    ("M13", "src/config.rs",
     "        let is = |word: &str| raw.eq_ignore_ascii_case(word);",
     "        let is = |word: &str| raw.trim().eq_ignore_ascii_case(word);",
     [WORDS_UNIT, SURFACES]),
    ("M14", "src/permission/slash.rs",
     "        let argument = argument.trim();",
     "        let argument = argument.trim_end_matches('\\u{0}');",
     [SLASH_TRIM]),
    ("M15", "src/config.rs",
     "        .filter(|raw| !raw.trim().is_empty())",
     "        .filter(|_| true)",
     [SURFACES_UNIT]),
    ("M16", "src/cli.rs",
     "        /// Disable xfx permission checks",
     "        /// Skip every permission check for this run (prints a warning)",
     [HELP]),
    ("M17", "src/app.rs",
     'format!("mode=full access: no permission check runs at all; sandbox={sandbox}")',
     'format!("mode=yolo: no permission check runs at all; sandbox={sandbox}")',
     [SPOKEN]),
    ("M18", "src/cli.rs",
     '        #[arg(long, conflicts_with_all = ["yolo", "full_access"])]',
     '        #[arg(long, conflicts_with = "yolo")]',
     [FLAGS_UNIT, FLAG]),
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
    print("TMPDIR is the caller's; each test run bounded at 900 s")
    for name, path, old, new, tests in MUTANTS:
        subprocess.run(["git", "-C", clone, "checkout", "-q", "--", "."], check=True)
        edit(clone, path, old, new)
        failed = []
        for target, test in tests:
            code, out = run(["cargo", "test", "--locked", "-q", *target, "--", test], clone, 900)
            if code != 0:
                tail = [line for line in out.splitlines() if "panicked at" in line or line.startswith("error")]
                failed.append(f"{target[-1]}::{test} ({code}; {tail[0].split(' at ')[-1] if tail else 'no panic line'})")
        verdict = "KILLED" if failed else "SURVIVED"
        print(f"{name} {verdict}: " + ("; ".join(failed) if failed else "every test passed"), flush=True)
    subprocess.run(["git", "-C", clone, "checkout", "-q", "--", "."], check=True)


if __name__ == "__main__":
    main()
