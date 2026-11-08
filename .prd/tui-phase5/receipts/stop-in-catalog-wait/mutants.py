#!/usr/bin/env python3
"""STOP-IN-CATALOG-WAIT's mutants, each on a throwaway clone (`git clone
--no-local`), never on the worktree.

    python3 -I -B mutants.py <clone> <label> <mutant>...

For each mutant: the clone's tracked files are reset (`git checkout -- .`),
the mutant's replacements are applied (each must match exactly once, or the
mutant is refused), and the tests below run under a 600 s limit (a process
group killed at the deadline). A mutant is `killed` when the run fails,
`SURVIVED` when it passes, `timeout` at the deadline. The clone is reset
after each. Prints one line per mutant, then the run's last lines.
"""
import os
import signal
import subprocess
import sys

TESTS = [
    "tui::worker::tests::a_stop_in_a_turns_catalog_wait",
    "agent::machine::tests::a_turn_stopped_before_it_began",
]

W = "src/tui/worker.rs"
M = "src/agent/machine.rs"
B = "src/tui/bridge.rs"

ARM = (
    "        // Stopped: nothing will be sent, so nothing is asked for, and the\n"
    "        // machine ends the turn as the stop (above).\n"
    "        Err(_) if turn.token.is_cancelled() => crate::gateway::protocol::RequestOptions::default(),\n"
)
SKIP = "    if carries_images && !turn.token.is_cancelled() {\n"
GUARD = (
    "        if self.request.cancel.is_cancelled() {\n"
    "            stopped = Some(TurnError::Cancelled);\n"
    "        } else if self.compactor.is_some() {\n"
)
EFFORT_END = (
    "            let _ = bridge::send_ui(events, &turn.token, state.catalog_event()).await;\n"
    "        }\n"
    "    }\n"
    "    // **A stop that came while the catalog was waited for is the turn's end,\n"
)

MUTANTS = {
    # Ultra's refusal no longer yields to the stop.
    "ultra-refusal-not-yielded": [(W, ARM, "")],
    # The image refusal is asked of a stopped turn.
    "image-refusal-asked-when-stopped": [(W, SKIP, "    if carries_images {\n")],
    # The brief's candidate shape: a bare return after the wait, nothing
    # recorded, and neither yield.
    "candidate-bare-return": [
        (W, ARM, ""),
        (W, SKIP, "    if carries_images {\n"),
        (
            W,
            EFFORT_END,
            EFFORT_END.replace(
                "    }\n    // **A stop",
                "    }\n    if turn.token.is_cancelled() {\n"
                "        return Some(bridge::TURN_CANCELLED.to_string());\n"
                "    }\n    // **A stop",
            ),
        ),
    ],
    # The machine measures a compaction for a turn stopped before it began.
    "no-machine-guard": [(M, GUARD, "        if self.compactor.is_some() {\n")],
    # A stop reaches the next turn: its mirror is not reset (gpt6's NTH pin).
    "stop-leaks-to-next-turn": [(B, "        self.mirror.reset();\n", "")],
}


def apply(clone, edits):
    for path, old, new in edits:
        full = os.path.join(clone, path)
        with open(full, encoding="utf-8") as handle:
            text = handle.read()
        count = text.count(old)
        if count != 1:
            raise SystemExit(f"{path}: the mutant's text matches {count} times")
        with open(full, "w", encoding="utf-8") as handle:
            handle.write(text.replace(old, new))


def run(clone, tests):
    env = dict(os.environ)
    for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
        env.pop(name, None)
    child = subprocess.Popen(
        ["cargo", "test", "--locked", "--lib", "--", *tests],
        cwd=clone,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        start_new_session=True,
        text=True,
    )
    try:
        out, _ = child.communicate(timeout=600)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        out, _ = child.communicate()
        return "timeout", out
    return ("SURVIVED" if child.returncode == 0 else "killed"), out


def main():
    clone, label, names = sys.argv[1], sys.argv[2], sys.argv[3:]
    tests = TESTS
    if names and names[0] == "--join-only":
        tests = ["tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_at_once"]
        names = names[1:]
    subprocess.run(["git", "-C", clone, "checkout", "--", "."], check=True)
    for name in names:
        apply(clone, MUTANTS[name])
        try:
            verdict, out = run(clone, tests)
        finally:
            subprocess.run(["git", "-C", clone, "checkout", "--", "."], check=True)
        print(f"{label} {name}: {verdict}")
        tail = [line for line in out.splitlines() if line.startswith(("test ", "test result", "thread "))]
        for line in tail[-8:]:
            print(f"    {line}")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
