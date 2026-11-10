"""CTRLO-OTHER-PICKERS' mutants (round 2): each picker's rule under Ctrl-O
broken one at a time in a throwaway clone (`git clone --no-local`), and the unit's modules
run against it with a time limit, in their own process group. A mutant the
tests fail is KILLED; one that does not build is NO-BUILD and not counted as
killed. (CTRLO-AT-KEYS' runner, `../ctrlo-at-keys/mutants.py`, with this
unit's mutants; its runner is AT-LAUNCH-WALK's.) Round 2 put the three
per-picker guards and the `@` list's inline copy into one check in
`consume_inputs`, so their mutants are one (`b-check-gone`), and
CTRLO-AT-KEYS' equivalent survivor has no line left to mutate; the
same-burst composer's `C-j` and the merged anchor have their own.

usage: python3 -I mutants.py <clone> [<mutant id>...]
The clone must be at the unit's head with no change; each mutant is restored
with `git checkout -- <file>` in the clone before the next. Prints one line per
mutant and `mutants: N run, K killed, B did not build`.
"""

import os
import signal
import subprocess
import sys

sys.dont_write_bytecode = True

# The modules whose cases read the pickers or the full transcript's keys.
TESTS = [
    "tui::shell",
    "tui::event_loop",
    "tui::full_transcript",
    "tui::file_picker",
    "tui::file_index",
]
LIMIT_S = 900

SH = "src/tui/shell.rs"

# The seven menus fx's frame shuts the transcript for.
MENUS = (
    "        let menu = self.settings_menu.is_some()\n"
    "            || self.help_menu.is_some()\n"
    "            || self.skills_menu.as_ref().is_some_and(SkillsMenu::visible)\n"
    "            || self.model_menu.is_some()\n"
    "            || self.sessions.is_some()\n"
    "            || self.fork_menu.is_some()\n"
    "            || self.effort_menu.is_some();\n"
)
CHECK = "            if !to_the_composer && self.offer_to_pickers(&event) {\n"
CONTROL = "        self.full.is_open() && matches!(action, Action::ControlJ | Action::ControlK)\n"
HELD = (
    "        let held = self\n"
    "            .skills_menu\n"
    "            .as_ref()\n"
    "            .is_some_and(|menu| !menu.origin().is_mention())\n"
    "            || self.model_menu.is_some()\n"
    "            || self.sessions.is_some();\n"
)
KEEP_CATALOG = "        if let (Some(menu), Some(mark)) = (self.help_menu.as_mut(), catalog) {\n            menu.keep(mark);\n        }\n"
KEEP_SETTINGS = "        if let (Some(menu), Some(mark)) = (self.settings_menu.as_mut(), settings) {\n            menu.keep(mark);\n        }\n"
ANCHOR_ROW = "        if row != self.editor.point(cols).0 {\n            return PROMPT_CELLS;\n        }\n"


def once(old, new):
    def mutate(text):
        if text.count(old) != 1:
            return None, f"the text to mutate is there {text.count(old)} times"
        return text.replace(old, new), None
    return mutate


def term_dropped(term):
    """The list without one menu: `false` in its place."""
    return once(MENUS, MENUS.replace(term, "false", 1))


def unguarded(name):
    """The picker's C-j/C-k guard taken out: it moves on them under the
    transcript again."""
    head = f"    fn {name}(&mut self, event: &Input) -> bool {{\n" + OPEN
    return once(head + GUARD, head)


MUTANTS = [
    # Group A: each menu fx's frame shuts the transcript for, dropped from the list
    ("a-settings-dropped", SH, term_dropped("self.settings_menu.is_some()")),
    ("a-help-dropped", SH, term_dropped("self.help_menu.is_some()")),
    ("a-skills-dropped", SH, term_dropped("self.skills_menu.as_ref().is_some_and(SkillsMenu::visible)")),
    ("a-model-dropped", SH, term_dropped("self.model_menu.is_some()")),
    ("a-sessions-dropped", SH, term_dropped("self.sessions.is_some()")),
    ("a-fork-dropped", SH, term_dropped("self.fork_menu.is_some()")),
    ("a-effort-dropped", SH, term_dropped("self.effort_menu.is_some()")),
    # the skills menu counted whenever it exists, shown or not (fx: menuVisible)
    ("a-skills-unshown-too", SH, once(MENUS, MENUS.replace(
        "self.skills_menu.as_ref().is_some_and(SkillsMenu::visible)",
        "self.skills_menu.is_some()",
    ))),
    # the whole rule reverted: no reconcile before the frame
    ("a-reconcile-gone", SH, once(
        "        self.reconcile_full_transcript();\n        // After the Escape",
        "        // After the Escape",
    )),
    # Group B and the burst: the one check that sends C-j/C-k under the
    # screen past every picker, taken out
    ("b-check-gone", SH, once(CHECK, "            if self.offer_to_pickers(&event) {\n")),
    # half reverted: only one of the two keys sent past the pickers
    ("b-cj-only", SH, once(CONTROL, CONTROL.replace("Action::ControlJ | Action::ControlK", "Action::ControlJ"))),
    ("b-ck-only", SH, once(CONTROL, CONTROL.replace("Action::ControlJ | Action::ControlK", "Action::ControlK"))),
    # the composer's C-j (fx's insert_newline): :514 gone, then each term
    ("burst-held-gone", SH, once(HELD + "        if held {\n            return;\n        }\n", HELD.replace("let held", "let _held"))),
    ("burst-held-no-skills", SH, once(HELD, HELD.replace(".is_some_and(|menu| !menu.origin().is_mention())", ".is_some_and(|_| false)"))),
    ("burst-held-no-model", SH, once(HELD, HELD.replace("|| self.model_menu.is_some()", "|| false"))),
    ("burst-held-no-sessions", SH, once(HELD, HELD.replace("|| self.sessions.is_some()", "|| false"))),
    # the catalog's and the settings menu's marks reset by the line break
    ("burst-catalog-mark-reset", SH, once(KEEP_CATALOG, "")),
    ("burst-settings-mark-reset", SH, once(KEEP_SETTINGS, "")),
    # the anchor as the base had it, now under every column that hangs from it
    ("anchor-row-check-gone", SH, once(ANCHOR_ROW, "")),
]


def run(clone, args, **kw):
    return subprocess.run(args, cwd=clone, **kw)


def main():
    clone = os.path.realpath(sys.argv[1])
    wanted = set(sys.argv[2:])
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    status = run(clone, ["git", "status", "--porcelain"], capture_output=True, text=True, env=env)
    if status.stdout.strip():
        sys.exit(f"the clone is not clean:\n{status.stdout}")
    head = run(clone, ["git", "rev-parse", "--short=8", "HEAD"], capture_output=True, text=True, env=env).stdout.strip()
    print(f"# clone at {head}; tests: cargo test --locked --lib -- {' '.join(TESTS)}; limit {LIMIT_S} s each", flush=True)
    ran = killed = broken = 0
    for mid, path, mutate in MUTANTS:
        if wanted and mid not in wanted:
            continue
        full = os.path.join(clone, path)
        with open(full, encoding="utf-8") as f:
            text = f.read()
        new, why = mutate(text)
        if new is None:
            print(f"BROKEN   {mid}: {why} ({path})", flush=True)
            continue
        with open(full, "w", encoding="utf-8") as f:
            f.write(new)
        ran += 1
        try:
            cmd = ["cargo", "test", "--locked", "--lib", "--", *TESTS]
            # Its own process group, so a run past the limit is stopped with
            # every process it started, and nothing else.
            child = subprocess.Popen(
                cmd, cwd=clone, env=env, text=True, start_new_session=True,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            )
            try:
                out, _ = child.communicate(timeout=LIMIT_S)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.communicate()
                raise
            failing = [line.split()[1] for line in out.splitlines() if line.startswith("test ") and line.endswith("FAILED")]
            summary = next((l for l in out.splitlines() if l.startswith("test result:")), "")
            if child.returncode == 0:
                verdict = "SURVIVED"
            elif not failing and not summary:
                broken += 1
                verdict = "NO-BUILD"
                failing = [next((l for l in out.splitlines() if l.startswith("error")), "build failed")]
            else:
                killed += 1
                verdict = "KILLED"
            print(f"{verdict:8} {mid}: {', '.join(failing) or 'every test passed'}  [{summary}]", flush=True)
        except subprocess.TimeoutExpired:
            killed += 1
            print(f"KILLED   {mid}: timed out after {LIMIT_S} s", flush=True)
        finally:
            run(clone, ["git", "checkout", "--", path], env=env, check=True)
    print(f"mutants: {ran} run, {killed} killed, {broken} did not build")


if __name__ == "__main__":
    main()
