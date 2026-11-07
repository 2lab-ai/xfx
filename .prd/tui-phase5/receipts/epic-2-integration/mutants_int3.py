"""INT3's mutants, each applied to a throwaway clone at the INT3 head (the
clone's working directory is the argument), one at a time, the named tests
run, the files restored."""
import subprocess, sys, os
clone = sys.argv[1]
M = [
 ("update makes the home its own way", "src/mcp/config.rs",
  "    crate::profile_home::ensure(dir, dir).map_err(|err| {\n        if crate::profile_home::Refused::of(&err).is_some() ||",
  "    fs::create_dir_all(dir).map_err(|_| EditError::Failed)?;\n    crate::profile_home::ensure(dir, dir).map_err(|err| {\n        if crate::profile_home::Refused::of(&err).is_some() ||",
  ["profile_home::tests::scan::no_product_code_makes_a_directory"]),
 ("update skips the home's check", "src/mcp/config.rs",
  "    crate::profile_home::ensure(dir, dir).map_err(|err| {\n        if crate::profile_home::Refused::of(&err).is_some() || err.kind() == io::ErrorKind::NotFound\n        {\n            EditError::Refused(err.to_string())\n        } else {\n            EditError::Failed\n        }\n    })?;\n",
  "",
  ["profile_home::tests::an_open_profile_home_is_refused", "profile_home::tests::scan::every_profile_writer"]),
 ("a resume keeps the held prompt", "src/tui/shell.rs",
  "                self.fork_prompt = None;\n                self.fork_prompt_held = None;\n                if let Some(sessions) = self.sessions.as_mut() {",
  "                if let Some(sessions) = self.sessions.as_mut() {",
  ["a_fork_prompt_held_under_the_session_picker_is_dropped_by_the_resume", "a_fork_still_on_its_way_when_another_session_is_resumed"]),
 ("/usage leaves /mcp's menu up", "src/tui/shell.rs",
  "        self.mcp_menu = None;\n        self.close_fork_menu();\n        self.close_usage();",
  "        self.close_fork_menu();\n        self.close_usage();",
  ["the_usage_screen_and_the_mcp_menu_close_each_other"]),
]
env = dict(os.environ, TMPDIR='/private/tmp/claude-501/ep2')
for name, rel, old, new, tests in M:
    path = os.path.join(clone, rel)
    orig = open(path).read()
    assert orig.count(old) == 1, (name, orig.count(old))
    open(path, 'w').write(orig.replace(old, new))
    for test in tests:
        r = subprocess.run(['gtimeout', '-k', '10', '900', 'cargo', 'test', '--locked', '--lib', '-q', '--', test],
                           cwd=clone, capture_output=True, text=True, env=env)
        out = (r.stdout + r.stderr).splitlines()
        why = next((l.strip() for l in out if 'panicked at' in l or l.startswith('error')), '')
        msg = next((l.strip() for l in out if l.startswith('assertion') or l.strip().startswith('left:')), '')
        print(f"{name} / {test}: {'FAILS' if r.returncode else 'passes (NOT caught)'} | {why[-80:]} | {msg[:110]}")
    open(path, 'w').write(orig)
