#!/usr/bin/env python3
"""HISTORY-NOTICE-STOP's mutants (rounds 1 and 2), each on a throwaway clone (`git clone
--no-local`), never on the worktree.

    python3 -I -B mutants.py <clone> <label> <mutant>...

For each mutant: the clone's tracked files are reset (`git checkout -- .`), the mutant's
replacements are applied (each must match exactly once, or the mutant is refused), and the tests
below run under a 600 s limit (a process group killed at the deadline). A mutant is `killed` when
the tests run and fail, `SURVIVED` when they pass, `timeout` at the deadline, and `build-failed`
when the mutant does not compile (no test ran: not a kill). The clone is reset after each.
Prints one line per mutant, then the run's last lines.
"""
import os
import signal
import subprocess
import sys

TESTS = [
    "tui::worker::tests::a_turn_stopped_before_its_request_leaves_the_history_notice",
    "interactive::tests::the_line_shell_says_each_history_notice_once",
    # Round 2.
    "interactive::tests::the_line_shell_leaves_unsaid_only_the_line_it_could_not_write",
    "tui::worker::tests::the_history_notice_ledger_goes_with_its_conversation",
    "tui::shell::tests::a_history_notice_the_screen_takes_is_painted_after_a_stop",
]

W = "src/tui/worker.rs"
I = "src/interactive.rs"
S = "src/tui/shell.rs"

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
    # Round 2. The line shell's base rule: every line found is recorded, then written.
    "line-shell-said-all-when-found": [
        (
            I,
            "    for notice in conversation.unsaid(notices) {\n" + LINE_WRITE + "    }\n",
            "    let found = conversation.unsaid(notices);\n"
            "    for notice in &found {\n"
            "        conversation.said(notice.clone());\n"
            "    }\n"
            "    for notice in found {\n"
            "        writeln!(out, \"{notice}\")?;\n"
            "    }\n",
        )
    ],
    # One ledger for the process: a notice said stays said in every later
    # conversation, a resumed one of the same session included.
    "ledger-outlives-its-conversation": [
        (
            I,
            "impl Conversation {\n",
            "static EVERY: std::sync::Mutex<std::collections::BTreeSet<String>> =\n"
            "    std::sync::Mutex::new(std::collections::BTreeSet::new());\n"
            "impl Conversation {\n",
        ),
        (I, FILTER, FILTER.replace("!self.said.contains(notice)", "!self.said.contains(notice) && !EVERY.lock().unwrap().contains(notice)")),
        (
            I,
            "        self.said.insert(notice);\n",
            "        EVERY.lock().unwrap().insert(notice.clone());\n        self.said.insert(notice);\n",
        ),
    ],
    # The screen drops a notice that arrives while a stop is in force.
    "screen-drops-notice-under-stop": [
        (
            S,
            "            UiEvent::Notice(notice) => self.announce(notice),\n",
            "            UiEvent::Notice(notice) => {\n"
            "                if !self.cancelling {\n"
            "                    self.announce(notice);\n"
            "                }\n"
            "            }\n",
        )
    ],
    # A reset's drain lets the old conversation's notice onto the new screen.
    "reset-drain-keeps-notice": [
        (S, "            | UiEvent::Notice(_)\n", ""),
        (S, "            | UiEvent::Undone(_)\n", "            | UiEvent::Undone(_)\n            | UiEvent::Notice(_)\n"),
    ],
    # A resume's replay keeps what the screen still owed the old conversation.
    "replay-keeps-owed-marks": [
        (
            S,
            "        // a tool group. None of it belongs above the session that replaces it.\n"
            "        self.pacer.forget();\n"
            "        self.marks.clear();\n",
            "        // a tool group. None of it belongs above the session that replaces it.\n"
            "        self.pacer.forget();\n",
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
    if child.returncode != 0 and ("error[E" in out or "could not compile" in out):
        return "build-failed", out
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
