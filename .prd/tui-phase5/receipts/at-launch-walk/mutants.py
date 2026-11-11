"""AT-LAUNCH-WALK's mutants: each rule this unit adds broken one at a time in
a throwaway clone (`git clone --no-local`), and the unit's tests run against
it with a time limit, in their own process group. A mutant the tests fail is
KILLED; one that does not build says so (`no test ran`) and is not counted
as killed by a test. (AT-SCOPE-INTERIM's runner, `../at-scope-interim/
mutants.py`, with this unit's tests and mutants, a mutation being a function
of the file's text so that a test-side mutant can restore a whole region.)

usage: python3 -I mutants.py <clone> [<mutant id>...]
The clone must be at the unit's head with no change; each mutant is restored
with `git checkout -- <file>` in the clone before the next. Prints one line per
mutant and `mutants: N run, K killed`.
"""

import os
import signal
import subprocess
import sys

sys.dont_write_bytecode = True

BASE = "f4aa7dcc"
# The modules the unit's runs filter on: AT-SCOPE-INTERIM's six, and
# `tui::tests` (the launch's `open`).
TESTS = [
    "tui::file_index",
    "tui::file_picker",
    "tui::shell",
    "tui::event_loop",
    "tui::check",
    "tui::frame",
    "tui::tests",
]
LIMIT_S = 900

FI = "src/tui/file_index.rs"
SH = "src/tui/shell.rs"
MOD = "src/tui/mod.rs"


def once(old, new):
    def mutate(text, _clone):
        if text.count(old) != 1:
            return None, f"the text to mutate is there {text.count(old)} times"
        return text.replace(old, new), None
    return mutate


def every(old, new):
    def mutate(text, _clone):
        if not text.count(old):
            return None, "the text to mutate is not there"
        return text.replace(old, new), None
    return mutate


# The tripwire region: from the first case AT-SCOPE-INTERIM's round 2 added
# there to the end of the second tick case, at the base and at the head.
BASE_FROM = "    #[test]\n    fn a_change_before_any_walk_lists_the_first_walks_prefix_with_no_launch_walk() {\n"
HEAD_FROM = "    #[test]\n    fn a_fresh_sessions_first_at_after_the_launch_walk_is_whole_shows_its_rows_at_once() {\n"
UNTIL = "    #[test]\n    fn a_path_list_in_the_interim_lists_its_folder() {\n"


def tripwires_restored(text, clone):
    """The base's tripwire cases, guards and expectations as they were,
    in place of the head's flipped ones."""
    base = subprocess.run(["git", "show", f"{BASE}:{SH}"], cwd=clone, capture_output=True, text=True, check=True).stdout
    if base.count(BASE_FROM) != 1 or text.count(HEAD_FROM) != 1 or text.count(UNTIL) != 1:
        return None, "a region marker is missing"
    old = text[text.index(HEAD_FROM):text.index(UNTIL)]
    new = base[base.index(BASE_FROM):base.index(UNTIL)]
    return text.replace(old, new), None


MUTANTS = [
    # the launch's call (`startFileIndex`, app_entry_runtime.zig:334@481104fd)
    ("no-launch-walk", MOD, once("    shell.start_file_index();\n}\n", "}\n")),
    # `ensureScopeEpoch` walks nothing
    ("no-launch-walk-in-files", FI, once(
        "            return;\n        }\n        self.start_walk();\n    }\n\n    /// A picker has just opened",
        "            return;\n        }\n    }\n\n    /// A picker has just opened")),
    # the launch walk keeps the launch's epoch (file_index.zig:361, :386);
    # this mutant gives it the epoch of now: a change while it is the first
    # index retags it
    ("launch-walk-current-epoch", FI, once(
        "        self.epoch = self.epoch.wrapping_add(1);\n        self.extra = extra;\n",
        "        self.epoch = self.epoch.wrapping_add(1);\n        self.extra = extra;\n"
        "        if self.index.is_none() {\n            if let Some(walk) = self.walk.as_mut() {\n"
        "                walk.epoch = self.epoch;\n            }\n        }\n")),
    # none for an empty root (app_workspace_runtime.zig:146, file_index.zig:358)
    ("launch-walk-on-an-empty-root", FI, once(
        "        if self.root.as_os_str().is_empty() {\n            return;\n        }\n        self.start_walk();\n",
        "        self.start_walk();\n")),
    # only from idle (file_index.zig:357)
    ("launch-walk-not-only-from-idle", FI, once(
        "        if self.index.is_some() || self.walk.is_some() || self.failed {\n            return;\n        }\n",
        "")),
    # the walk is the launch's, not the constructor's (fx's App.init walks nothing)
    ("walk-in-the-constructor", SH, once(
        "                    &WorkspaceAccess::load(&config.workspace_root, &config.additional_directories),\n                ));\n                files\n",
        "                    &WorkspaceAccess::load(&config.workspace_root, &config.additional_directories),\n                ));\n                files.start_index();\n                files\n")),
    # round 2: the first `@`'s decision takes the walk's end only with no
    # index adopted -- fx's frame for a new scope's walk behind the launch
    # index (one frame of `Files unavailable`), where xfx records its own
    ("take-end-only-without-an-index", FI, once(
        "            self.episode_seen = true;\n            self.take_walk();\n",
        "            self.episode_seen = true;\n            if self.index.is_none() {\n                self.take_walk();\n            }\n")),
    # round 2: AT-SCOPE-INTERIM round 3's tick, the decision before the walk's end
    ("decision-before-the-walks-end", FI, once(
        "            self.episode_seen = true;\n            self.take_walk();\n",
        "            self.episode_seen = true;\n")),
    # round 2: the launch walk with the epoch before the launch's change
    ("launch-walk-stale-epoch", FI, once(
        "            return;\n        }\n        self.start_walk();\n    }\n\n    /// A picker has just opened",
        "            return;\n        }\n        self.start_walk();\n        if let Some(walk) = self.walk.as_mut() {\n"
        "            walk.epoch = self.epoch.wrapping_sub(1);\n        }\n    }\n\n    /// A picker has just opened")),
    # round 2: the launch walk's scope without the launch's directories
    ("launch-scope-without-the-saved-directories", SH, once(
        "                    &WorkspaceAccess::load(&config.workspace_root, &config.additional_directories),\n                ));\n                files\n",
        "                    &WorkspaceAccess::load(&config.workspace_root, &[]),\n                ));\n                files\n")),
    # test-side: the tripwires' guards and expectations put back as the base had them
    ("tripwire-cases-restored", SH, tripwires_restored),
    # test-side: the flipped cases with no launch walk driven
    ("flipped-cases-without-the-launch-walk", SH, every("        shell.start_file_index();\n", "")),
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
    ran = killed = 0
    for mid, path, mutate in MUTANTS:
        if wanted and mid not in wanted:
            continue
        full = os.path.join(clone, path)
        with open(full, encoding="utf-8") as f:
            text = f.read()
        new, why = mutate(text, clone)
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
            done = subprocess.CompletedProcess(cmd, child.returncode)
            if done.returncode == 0:
                verdict = "SURVIVED"
            else:
                killed += 1
                verdict = "KILLED"
            failing = [line.split()[1] for line in out.splitlines() if line.startswith("test ") and line.endswith("FAILED")]
            if done.returncode != 0 and not failing:
                failing = ["(no test ran: " + next((l for l in out.splitlines() if l.startswith("error")), "build failed") + ")"]
            summary = next((l for l in out.splitlines() if l.startswith("test result:")), "")
            print(f"{verdict:8} {mid}: {', '.join(failing) or 'every test passed'}  [{summary}]", flush=True)
        except subprocess.TimeoutExpired:
            killed += 1
            print(f"KILLED   {mid}: timed out after {LIMIT_S} s", flush=True)
        finally:
            run(clone, ["git", "checkout", "--", path], env=env, check=True)
    print(f"mutants: {ran} run, {killed} killed")


if __name__ == "__main__":
    main()
