"""INT2c's mutants, each applied to a throwaway clone's src/tui/shell.rs (the
clone's working directory is the argument), one at a time, the named test
run, the file restored."""
import subprocess, sys, os
clone = sys.argv[1]
P = os.path.join(clone, 'src/tui/shell.rs')
orig = open(P).read()
SKL = "            || self\n                .skills_menu"
M = [
 ("no clear on a session switch", "        self.fork_prompt_held = None;\n        self.tally = Tally::default();", "        self.tally = Tally::default();", "a_fork_prompt_held_under_the_session_picker"),
 ("C-p give-back drops the prompt", "        self.fork_prompt = prompt;\n        self.fork_prompt_held = held;\n", "", "a_ctrl_p_model_menu_opened"),
 ("no settle before a key", "            // And a landed fork's prompt held while a search had the composer.\n            self.settle_fork_prompt();\n", "", "a_key_read_with_the_close"),
 ("no empty-composer guard", "        if self.editor.is_empty() {\n            self.edit_history.boundary();\n            if self\n                .editor\n                .set_text(&super::bridge::inert(&prompt), Entities::new())", "        if true {\n            self.edit_history.boundary();\n            if self\n                .editor\n                .set_text(&super::bridge::inert(&prompt), Entities::new())", "a_held_forks_prompt_never"),
 ("no reset clear", "        self.fork_prompt = None;\n        self.fork_prompt_held = None;\n        self.raw_gap = false;", "        self.fork_prompt = None;\n        self.raw_gap = false;", "a_held_forks_prompt_never"),
 ("mcp not a search", "        self.mcp_menu.is_some()\n            || self.help_menu.is_some()", "        self.help_menu.is_some()", "a_fork_that_lands_under_a_search"),
 ("catalog not a search", "        self.mcp_menu.is_some()\n            || self.help_menu.is_some()", "        self.mcp_menu.is_some()", "a_fork_that_lands_under_a_search"),
 ("settings not a search", "            || self.settings_menu.is_some()\n            || self.model_menu.is_some()\n            || self.sessions.is_some()\n" + SKL, "            || self.model_menu.is_some()\n            || self.sessions.is_some()\n" + SKL, "a_fork_that_lands_under_a_search"),
 ("model menu not a search", "            || self.settings_menu.is_some()\n            || self.model_menu.is_some()\n            || self.sessions.is_some()\n" + SKL, "            || self.settings_menu.is_some()\n            || self.sessions.is_some()\n" + SKL, "a_fork_that_lands_under_a_search"),
 ("sessions not a search", "            || self.sessions.is_some()\n" + SKL, SKL, "a_fork_that_lands_under_a_search"),
 ("skills not a search", "            || self.sessions.is_some()\n" + SKL + "\n                .as_ref()\n                .is_some_and(|menu| !menu.origin().is_mention())\n    }", "            || self.sessions.is_some()\n    }", "a_fork_that_lands_under_a_search"),
]
env = dict(os.environ, TMPDIR='/private/tmp/claude-501/ep2')
for name, old, new, test in M:
    assert orig.count(old) == 1, (name, orig.count(old))
    open(P, 'w').write(orig.replace(old, new))
    r = subprocess.run(['gtimeout', '-k', '10', '900', 'cargo', 'test', '--locked', '--lib', '-q', '--', test],
                       cwd=clone, capture_output=True, text=True, env=env)
    out = (r.stdout + r.stderr).splitlines()
    why = next((l.strip() for l in out if l.startswith('assertion') or l.startswith('/') or 'opened' in l or l.startswith('  left')), '')
    msg = [l for l in out if l.strip().startswith(('left:', 'right:'))]
    print(f"{name}: {'FAILS' if r.returncode else 'passes (NOT caught)'} | {why[:110]} {' '.join(m.strip() for m in msg)[:120]}")
open(P, 'w').write(orig)
