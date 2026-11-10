"""The unit's mutants, each on a throwaway clone (`git clone --no-local` of the
head into <clone>, never the worktree): one edit of the head's source, then
the tests that must catch it, with a bounded run. A mutant is KILLED when at
least one of its tests fails, SURVIVED when all pass. The clone is reset
between mutants.

    M1  the unreadable / malformed / too-large file no longer refuses
    M2  a `workspaces` or entry that is not an object no longer refuses
    M3  the profile's `model` no longer refuses
    M4  the workspace entry's `model` no longer refuses
    M5  fx's model rule loosened: a blank around the id is allowed
    M6  `skill_match_fuzzy` no longer shields the model (refuses what fx starts)
    M7  the unsafe path no longer refuses `ask`
    M8  the unsafe path refuses the interactive launches too (a refusal fx's
        stored-credential launch does not make: xfx's analogue of refusing in
        a non-local auth mode -- xfx has no auth mode, so the one mode split
        fx makes inside the local mode is the one there is to break)
    M9  the TUI refuses after the terminal is taken (the check moved into
        `session`, after raw mode)
    M10 the line shell refuses after its session store is opened
    M11 `ask --json` writes the refusal on stderr, not as its error event
    M12 the project file's malformed content refuses too
    -- round 2, fx's read order (`profile_open`):
    M13 the file's mode no longer checked: a file group may write is read
    M14 the directory's mode no longer checked
    M15 the open no longer checked before the mode: a 0o222 file, which fx
        refuses at the open, is taken as one it does not read
    M16 a second hard link no longer unsafe
    M17 a `~/.xfx` that does not open no longer refused
    (Dropping the regular-file check is not run: its FIFO case would block
    in the open the check exists to prevent.)

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

MUTANTS = [
    ("M1", "src/config.rs",
     "let mut profile_rejected = user.present && !user_settings_loaded;",
     "let mut profile_rejected = false;",
     [(CLI, "ask_refuses_a_profile_fx_refuses"), (LIB, "config::tests::the_refusal_is_read")]),
    ("M2", "src/config.rs",
     "                Err(reason) => {\n                    profile_rejected = true;\n",
     "                Err(reason) => {\n",
     [(CLI, "ask_refuses_a_profile_fx_refuses"), (LIB, "config::tests::the_refusal_is_read")]),
    ("M3", "src/config.rs",
     "            profile_rejected |= fx_refuses_model(object);\n",
     "",
     [(CLI, "ask_refuses_a_profile_fx_refuses")]),
    ("M4", "src/config.rs",
     "                profile_rejected |= fx_refuses_model(entry);\n",
     "",
     [(CLI, "ask_refuses_a_profile_fx_refuses"), (LIB, "config::tests::the_refusal_is_read")]),
    ("M5", "src/config.rs",
     "                || model.trim_matches([' ', '\\t', '\\r', '\\n']) != model\n",
     "",
     [(LIB, "config::tests::a_launch_refuses_the_models"), (CLI, "ask_refuses_a_profile_fx_refuses")]),
    ("M6", "src/config.rs",
     '    if object.contains_key("skill_match_fuzzy") {\n        return false;\n    }\n',
     "",
     [(LIB, "config::tests::a_launch_refuses_the_models"), (CLI, "ask_goes_on_with_a_profile_fx_starts_with")]),
    ("M7", "src/config.rs",
     "            Some(ProfileRefusal::Unsafe) => launch == Launch::Ask,",
     "            Some(ProfileRefusal::Unsafe) => false,",
     [(CLI, "ask_refuses_a_profile_fx_refuses"), (LIB, "config::tests::the_refusal_is_read")]),
    ("M8", "src/config.rs",
     "            Some(ProfileRefusal::Unsafe) => launch == Launch::Ask,",
     "            Some(ProfileRefusal::Unsafe) => true,",
     [(SHELL, "the_shell_starts_with_a_profile_only_ask_refuses"),
      (TUI, "a_profile_only_ask_refuses_starts_the_tui"), (LIB, "config::tests::the_refusal_is_read")]),
    ("M9", "src/tui/mod.rs",
     "    if config.refuses_launch(crate::config::Launch::Interactive) {\n        return fail(crate::config::PROFILE_REFUSAL);\n    }\n",
     "",
     [(TUI, "a_profile_fx_refuses_is_refused_before_the_terminal_is_taken")],
     # ... and the check put back after raw mode, in `session`:
     ("src/tui/mod.rs", "    let mut shell = shell::Shell::new(config, geometry, palette, theme_locked, worker.handle());\n",
      "    if config.refuses_launch(crate::config::Launch::Interactive) {\n        return Err(io::Error::other(crate::config::PROFILE_REFUSAL));\n    }\n"
      "    let mut shell = shell::Shell::new(config, geometry, palette, theme_locked, worker.handle());\n")),
    ("M10", "src/interactive.rs",
     "    if config.refuses_launch(crate::config::Launch::Interactive) {\n        writeln!(diagnostics, \"xfx: {}\", crate::config::PROFILE_REFUSAL)?;\n        return Ok(ExitCode::from(1));\n    }\n",
     "",
     [(SHELL, "the_shell_refuses_a_profile_fx_refuses_before_it_writes_anything")],
     ("src/interactive.rs", "    if config.permission_mode == PermissionMode::Yolo {\n        writeln!(diagnostics, \"{YOLO_WARNING}\")?;\n    }\n    // What the configuration could not use, as `xfx ask` says it\n",
      "    if config.refuses_launch(crate::config::Launch::Interactive) {\n        writeln!(diagnostics, \"xfx: {}\", crate::config::PROFILE_REFUSAL)?;\n        return Ok(ExitCode::from(1));\n    }\n"
      "    if config.permission_mode == PermissionMode::Yolo {\n        writeln!(diagnostics, \"{YOLO_WARNING}\")?;\n    }\n    // What the configuration could not use, as `xfx ask` says it\n")),
    ("M11", "src/app.rs",
     "        if request.json {\n            let mut sink = JsonlSink::new(stdout).on_terminal(stdout_is_terminal);\n            return fail_turn(&mut sink, crate::config::PROFILE_REFUSAL.to_string());\n        }\n",
     "",
     [(CLI, "ask_refuses_a_profile_fx_refuses")]),
    ("M12", "src/config.rs",
     "        let project_settings_present = project.present;\n",
     "        let project_settings_present = project.present;\n        let project_rejected = project.present && project.object.is_none();\n",
     [(CLI, "ask_goes_on_with_a_profile_fx_starts_with"), (LIB, "config::tests::the_refusal_is_read")],
     ("src/config.rs", "        let mut profile_rejected = user.present && !user_settings_loaded;\n",
      "        let mut profile_rejected = (user.present && !user_settings_loaded) || project_rejected;\n")),
    ("M13", "src/config.rs",
     "    if open_to_others(&metadata) {\n        return ProfileOpen::NotRead;\n    }\n",
     "",
     [(CLI, "ask_goes_on_with_a_profile_fx_starts_with"), (LIB, "config::tests::the_profile_is_opened")]),
    ("M14", "src/config.rs",
     "    if open_to_others(&dir_metadata) {\n        return ProfileOpen::NotRead;\n    }\n",
     "",
     [(CLI, "ask_goes_on_with_a_profile_fx_starts_with"), (LIB, "config::tests::the_profile_is_opened")]),
    ("M15", "src/config.rs",
     "    if fs::File::open(file).is_err() {\n        return ProfileOpen::Unopenable;\n    }\n",
     "",
     [(CLI, "ask_refuses_a_profile_fx_refuses"), (LIB, "config::tests::the_profile_is_opened")]),
    ("M16", "src/config.rs",
     "    if !metadata.is_file() || hard_links(&metadata) > 1 {",
     "    if !metadata.is_file() {",
     [(CLI, "ask_refuses_a_profile_fx_refuses"), (LIB, "config::tests::the_profile_is_opened")]),
    ("M17", "src/config.rs",
     "    if fs::read_dir(dir).is_err() {\n        return ProfileOpen::Unopenable;\n    }\n",
     "",
     [(LIB, "config::tests::the_profile_is_opened")]),
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
