"""AT-SCOPE-INTERIM's mutants: each rule this unit adds, broken one at a time
in a throwaway clone (`git clone --no-local`), and the unit's tests run
against it with a time limit, in their own process group. A mutant the tests
fail is KILLED; one that does not build says so (`no test ran`) and is not
counted as killed by a test. (The runner is AT-FOLDERS round 9's,
`../at-folders/round9/mutants.py`, with this unit's tests and mutants.)

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

# The files the unit touches, whole: the same filter as the unit's runs.
TESTS = [
    "tui::file_index",
    "tui::file_picker",
    "tui::shell",
    "tui::event_loop",
    "tui::check",
    "tui::frame",
]
LIMIT_S = 900

FI = "src/tui/file_index.rs"
SH = "src/tui/shell.rs"

INDEX_ARM = "                (Some(_), _) if self.index_epoch != self.epoch => Lookup::Unavailable,\n"
PREFIX_ARM = "                (None, Some(walk)) if walk.epoch != self.epoch => Lookup::Unavailable,\n"

MUTANTS = [
    # the epoch check (`prepareFilePicker`, input_completion_runtime.zig:887-888@481104fd)
    ("no-epoch-check", FI, INDEX_ARM + "                (Some(index), _) => Lookup::Rows(search(index, query)),\n" + PREFIX_ARM,
     "                (Some(index), _) => Lookup::Rows(search(index, query)),\n"),
    ("no-epoch-check-on-the-index", FI, INDEX_ARM, ""),
    ("no-epoch-check-on-the-prefix", FI, PREFIX_ARM, ""),
    # the old text: the interim said `indexing files...`
    ("old-text-kept", FI, INDEX_ARM + "                (Some(index), _) => Lookup::Rows(search(index, query)),\n" + PREFIX_ARM,
     INDEX_ARM.replace("Unavailable", "Loading") + "                (Some(index), _) => Lookup::Rows(search(index, query)),\n" + PREFIX_ARM.replace("Unavailable", "Loading")),
    # every change of the access moves the epoch, not only a change of roots
    ("epoch-only-on-root-change", FI, "        self.epoch = self.epoch.wrapping_add(1);\n        self.extra = extra;",
     "        if extra != self.extra {\n            self.epoch = self.epoch.wrapping_add(1);\n        }\n        self.extra = extra;"),
    # the old model: a change stops the walk, or drops the index
    ("stop-walk-on-change", FI, "        self.epoch = self.epoch.wrapping_add(1);\n        self.extra = extra;",
     "        self.epoch = self.epoch.wrapping_add(1);\n        self.extra = extra;\n        if let Some(walk) = self.walk.take() {\n            walk.stop.store(true, Atomic::Relaxed);\n        }"),
    ("drop-index-on-change", FI, "        self.epoch = self.epoch.wrapping_add(1);\n        self.extra = extra;",
     "        self.epoch = self.epoch.wrapping_add(1);\n        self.extra = extra;\n        self.index = None;"),
    # a walk is of the scope it started in, not the one it ends in
    ("adopted-as-the-current-scope", FI, "                    self.index_epoch = walk.epoch;",
     "                    self.index_epoch = self.epoch;"),
    # fx's "loading": no index adopted yet, and a walk running
    ("loading-ignores-the-index", FI, "        self.index.is_none() && self.walk.is_some()", "        self.walk.is_some()"),
    # a path list is not the index's: its source is the app's own epoch (input_completion_runtime.zig:867)
    ("epoch-check-on-a-path-list", FI, "        let Some(directory) = resolve(&self.root, self.home.as_deref(), explicit.parent) else {\n",
     "        if self.index.is_some() && self.index_epoch != self.epoch {\n            return Lookup::Unavailable;\n        }\n        let Some(directory) = resolve(&self.root, self.home.as_deref(), explicit.parent) else {\n"),
    # round 2: the two cases that pin xfx's side of a named difference
    ("walk-at-launch", SH, "                    &WorkspaceAccess::load(&config.workspace_root, &config.additional_directories),\n                ));\n                files\n",
     "                    &WorkspaceAccess::load(&config.workspace_root, &config.additional_directories),\n                ));\n                files.refresh_index();\n                files\n"),
    ("never-loading", FI, "        self.index.is_none() && self.walk.is_some()", "        false"),
    # round 3: the first `@`'s decision reads a finished walk, as fx's currentState does
    ("decision-before-the-walks-end", FI, "            self.episode_seen = true;\n            self.take_walk();\n",
     "            self.episode_seen = true;\n"),
    # the shell moves the epoch on every change it sees
    ("no-scope-change-in-adopt", SH, "            self.files\n                .change_scope(index_roots(&self.workspace, &access));",
     "            let _ = index_roots(&self.workspace, &access);"),
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
    ran = killed = 0
    for mid, path, old, new in MUTANTS:
        if wanted and mid not in wanted:
            continue
        full = os.path.join(clone, path)
        with open(full, encoding="utf-8") as f:
            text = f.read()
        if text.count(old) != 1:
            print(f"BROKEN   {mid}: the text to mutate is in {path} {text.count(old)} times")
            continue
        with open(full, "w", encoding="utf-8") as f:
            f.write(text.replace(old, new))
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
            print(f"{verdict:8} {mid}: {', '.join(failing) or 'every test passed'}", flush=True)
        except subprocess.TimeoutExpired:
            killed += 1
            print(f"KILLED   {mid}: timed out after {LIMIT_S} s", flush=True)
        finally:
            run(clone, ["git", "checkout", "--", path], env=env, check=True)
    print(f"mutants: {ran} run, {killed} killed")


if __name__ == "__main__":
    main()
