#!/usr/bin/env python3
"""HISTORY-NOTICE-STOP's mutants, each on a throwaway clone (`git clone --no-local`), never on
the worktree.

    python3 -I -B mutants.py <clone> <label> <mutant>...

For each mutant: the clone's tracked files are reset (`git checkout -- .`), the mutant's
replacements are applied (each must match exactly once, or the mutant is refused), and the tests
below run under a 600 s limit (a process group killed at the deadline). A mutant is `killed` when
the run fails, `SURVIVED` when it passes, `timeout` at the deadline. The clone is reset after
each. Prints one line per mutant, then the run's last lines.
"""
import os
import signal
import subprocess
import sys

TESTS = [
    "tui::worker::tests::a_turn_stopped_before_its_request_leaves_the_history_notice",
    "interactive::tests::the_line_shell_says_each_history_notice_once",
]

W = "src/tui/worker.rs"
I = "src/interactive.rs"

TUI_SEND = (
    "        if bridge::send_ui(events, &turn.token, UiEvent::Notice(shown))\n"
    "            .await\n"
    "            .is_ok()\n"
    "        {\n"
    "            conversation.said(notice);\n"
    "        }\n"
)
LINE_WRITE = (
    "        writeln!(out, \"{notice}\")?;\n"
    "        conversation.said(notice);\n"
)
FILTER = "            .filter(|notice| !self.said.contains(notice) && found.insert(notice.clone()))\n"

MUTANTS = {
    # The base's rule: said when found, before the send, whatever it does.
    "tui-said-when-found": [
        (
            W,
            TUI_SEND,
            "        conversation.said(notice);\n"
            "        let _ = bridge::send_ui(events, &turn.token, UiEvent::Notice(shown)).await;\n",
        )
    ],
    # Said after the send, whether or not the send was refused.
    "tui-said-even-if-refused": [
        (
            W,
            TUI_SEND,
            "        let _ = bridge::send_ui(events, &turn.token, UiEvent::Notice(shown)).await;\n"
            "        conversation.said(notice);\n",
        )
    ],
    # Never said: the TUI repeats the notice on every turn.
    "tui-never-said": [
        (
            W,
            TUI_SEND,
            "        let _ = bridge::send_ui(events, &turn.token, UiEvent::Notice(shown)).await;\n"
            "        let _ = notice;\n",
        )
    ],
    # A line the rebuild repeats is found twice.
    "no-dedup-in-one-rebuild": [
        (I, FILTER, "            .filter(|notice| !self.said.contains(notice))\n"),
        (I, "        let mut found = std::collections::BTreeSet::new();\n", ""),
    ],
    # The line shell never records what it wrote.
    "line-shell-never-said": [(I, LINE_WRITE, "        writeln!(out, \"{notice}\")?;\n")],
    # The line shell records a line before writing it.
    "line-shell-said-before-written": [
        (
            I,
            LINE_WRITE,
            "        conversation.said(notice.clone());\n        writeln!(out, \"{notice}\")?;\n",
        )
    ],
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
    subprocess.run(["git", "-C", clone, "checkout", "--", "."], check=True)
    for name in names or list(MUTANTS):
        apply(clone, MUTANTS[name])
        try:
            verdict, out = run(clone, TESTS)
        finally:
            subprocess.run(["git", "-C", clone, "checkout", "--", "."], check=True)
        print(f"{label} {name}: {verdict}")
        tail = [line for line in out.splitlines() if line.startswith(("test ", "test result", "thread ", "error"))]
        for line in tail[-8:]:
            print(f"    {line}")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
