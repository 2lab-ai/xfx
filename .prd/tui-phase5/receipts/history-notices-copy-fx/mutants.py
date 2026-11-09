#!/usr/bin/env python3
"""HISTORY-NOTICES-COPY-FX's mutants, each on a throwaway clone (`git clone --no-local`), never
on the worktree.

    python3 -I -B mutants.py <clone> <label> <mutant>...

For each mutant: the clone's tracked files are reset (`git checkout -- .`), the mutant's
replacements are applied (each must match exactly once, or the mutant is refused), and each of
the mutant's test targets runs under a 900 s limit (a process group killed at the deadline). A
target is `killed` when its tests run and fail, `SURVIVED` when they pass, `timeout` at the
deadline, and `build-failed` when the mutant does not compile (no test ran: not a kill). The
clone is reset after each mutant. Prints one line per mutant and target, then its failing tests.
"""
import os
import signal
import subprocess
import sys

STORE = "src/session/store.rs"
APP = "src/app.rs"
W = "src/tui/worker.rs"
I = "src/interactive.rs"
S = "src/tui/shell.rs"

# The store, where the deleted line was pushed.
STATE = (
    "                let state: &[serde_json::Value] = if same_model { on_wire } else { &[] };\n"
    "                pending.push(if state.is_empty() {\n"
)
WIRE_TEXT = (
    "format!(\n"
    "                            \"xfx: this session recorded reasoning on the {} wire and is \\\n"
    "                             continuing on {}, so that reasoning was not carried over\",\n"
    "                            recorded.label(),\n"
    "                            active.label()\n"
    "                        )"
)
MODEL_TEXT = (
    "format!(\n"
    "                            \"xfx: this session recorded reasoning from {sealed} and is \\\n"
    "                             continuing with {model}, so that reasoning was not carried over\"\n"
    "                        )"
)
DROPPED = "                if state.is_empty() && !(raw_content.is_empty() && responses_state.is_empty()) {\n"


def back(arms):
    return [(STORE, STATE, STATE.replace("                pending.push", DROPPED + arms + "                }\n                pending.push"))]


# The tests that read the deleted line: lib, `tests/sessions.rs`, `tests/tui.rs` (pty).
SILENT = {
    "lib": ["session::store::carry_tests::"],
    "sessions": [
        "ask_resume_never_adopts",
        "a_legacy_record_is_dropped_without_a_word",
        "responses_state_recorded_under_one_authority",
        "a_wire_this_version_does_not_know",
        "a_wire_less_responses_state",
    ],
    "tui": [
        "a_switch_off_llmux_sends_its_text_and_calls",
        "after_a_model_change_on_llmux_no_block",
    ],
}
# Round 2: `xfx ask --resume`'s notice output, re-based on N5 with a hostile tool name.
ASK = {"sessions": ["ask_resume_never_adopts_a_model_with_a_control_character_or_prints_one"]}
ASK_LINE = "            crate::output::safe_one_line(notice, MAX_NOTICE_BYTES)\n"
# The once-per-conversation tests, re-based on an image notice.
ONCE = {
    "lib": [
        "tui::worker::tests::a_turn_stopped_before_its_request_leaves_the_history_notice",
        "tui::worker::tests::the_history_notice_ledger_goes_with_its_conversation",
        "tui::worker::tests::a_stop_through_a_dropped_compaction",
        "tui::shell::tests::a_history_notice_the_screen_takes_is_painted_after_a_stop",
        "interactive::tests::the_line_shell_says_each_history_notice_once",
        "interactive::tests::the_line_shell_leaves_unsaid_only_the_line_it_could_not_write",
    ]
}

TUI_SEND = (
    "        if bridge::send_ui(events, &turn.token, UiEvent::Notice(shown))\n"
    "            .await\n"
    "            .is_ok()\n"
    "        {\n"
    "            conversation.said(notice);\n"
    "        }\n"
)
LINE_LOOP = (
    "    for notice in conversation.unsaid(notices) {\n"
    "        writeln!(out, \"{notice}\")?;\n"
    "        conversation.said(notice);\n"
    "    }\n"
)
FILTER = "            .filter(|notice| !self.said.contains(notice) && found.insert(notice.clone()))\n"

MUTANTS = {
    # The deleted line brought back, both of its texts.
    "reasoning-notice-back": (
        back(
            "                    notices.push(match sealed_by {\n"
            "                        Some(sealed) if !on_wire.is_empty() => " + MODEL_TEXT + ",\n"
            "                        _ => " + WIRE_TEXT + ",\n"
            "                    });\n"
        ),
        SILENT,
    ),
    # Only its wire text: a switch of provider says it, a model change does too.
    "reasoning-notice-back-wire-only": (
        back("                    notices.push(" + WIRE_TEXT + ");\n"),
        SILENT,
    ),
    # Only its model text, and only for a model change on the same wire.
    "reasoning-notice-back-model-only": (
        back(
            "                    if let Some(sealed) = sealed_by.as_ref().filter(|_| !on_wire.is_empty()) {\n"
            "                        notices.push(" + MODEL_TEXT.replace("{sealed}", "{sealed}") + ");\n"
            "                    }\n"
        ),
        SILENT,
    ),
    # Round 2 (opus MUST 2): `ask` prints the log's words as they are.
    "ask-notice-not-made-safe": ([(APP, ASK_LINE, "            notice\n")], ASK),
    # Round 2: `ask` prints no history notice at all.
    "ask-notice-not-printed": (
        [(APP, "    for notice in &opened.notices {\n", "    for notice in opened.notices.iter().take(0) {\n")],
        ASK,
    ),
    # HISTORY-NOTICE-STOP's optional NTH (opus r2): the line shell marks only after every line
    # is written, so a write that fails after the first line leaves the first unsaid too.
    "line-shell-marks-after-every-line": (
        [
            (
                I,
                LINE_LOOP,
                "    let found = conversation.unsaid(notices);\n"
                "    for notice in &found {\n"
                "        writeln!(out, \"{notice}\")?;\n"
                "    }\n"
                "    for notice in found {\n"
                "        conversation.said(notice);\n"
                "    }\n",
            )
        ],
        ONCE,
    ),
    # HISTORY-NOTICE-STOP's mutants of the mechanism, against the re-based tests.
    "tui-said-when-found": (
        [
            (
                W,
                TUI_SEND,
                "        conversation.said(notice);\n"
                "        let _ = bridge::send_ui(events, &turn.token, UiEvent::Notice(shown)).await;\n",
            )
        ],
        ONCE,
    ),
    "tui-said-even-if-refused": (
        [
            (
                W,
                TUI_SEND,
                "        let _ = bridge::send_ui(events, &turn.token, UiEvent::Notice(shown)).await;\n"
                "        conversation.said(notice);\n",
            )
        ],
        ONCE,
    ),
    "tui-never-said": (
        [
            (
                W,
                TUI_SEND,
                "        let _ = bridge::send_ui(events, &turn.token, UiEvent::Notice(shown)).await;\n"
                "        let _ = notice;\n",
            )
        ],
        ONCE,
    ),
    "no-dedup-in-one-rebuild": (
        [
            (I, FILTER, "            .filter(|notice| !self.said.contains(notice))\n"),
            (I, "        let mut found = std::collections::BTreeSet::new();\n", ""),
        ],
        ONCE,
    ),
    "ledger-outlives-its-conversation": (
        [
            (
                I,
                "impl Conversation {\n",
                "static EVERY: std::sync::Mutex<std::collections::BTreeSet<String>> =\n"
                "    std::sync::Mutex::new(std::collections::BTreeSet::new());\n"
                "impl Conversation {\n",
            ),
            (
                I,
                FILTER,
                FILTER.replace(
                    "!self.said.contains(notice)",
                    "!self.said.contains(notice) && !EVERY.lock().unwrap().contains(notice)",
                ),
            ),
            (
                I,
                "        self.said.insert(notice);\n",
                "        EVERY.lock().unwrap().insert(notice.clone());\n        self.said.insert(notice);\n",
            ),
        ],
        ONCE,
    ),
    "screen-drops-notice-under-stop": (
        [
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
        ONCE,
    ),
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


def run(clone, target, tests):
    env = dict(os.environ)
    for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
        env.pop(name, None)
    which = ["--lib"] if target == "lib" else ["--test", target]
    child = subprocess.Popen(
        ["cargo", "test", "--locked", *which, "--", *tests],
        cwd=clone,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        start_new_session=True,
        text=True,
    )
    try:
        out, _ = child.communicate(timeout=900)
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
        edits, targets = MUTANTS[name]
        apply(clone, edits)
        try:
            for target, tests in targets.items():
                verdict, out = run(clone, target, tests)
                print(f"{label} {name} [{target}]: {verdict}")
                lines = out.splitlines()
                tail = []
                for at, line in enumerate(lines):
                    if line.startswith(("test result", "error")) or (
                        line.startswith("test ") and line.endswith("FAILED")
                    ):
                        tail.append(line)
                    elif line.startswith("thread ") and "panicked at" in line:
                        # The panic and its message: an assert_eq!'s left and right, or
                        # an assert!'s own message (round 2, opus SHOULD 3).
                        tail.append(line)
                        for follow in lines[at + 1 : at + 6]:
                            if not follow.strip() or follow.startswith(("note:", "thread ", "test ", "----")):
                                break
                            tail.append("  " + follow[:400])
                for line in tail[-24:]:
                    print(f"    {line}")
                sys.stdout.flush()
        finally:
            subprocess.run(["git", "-C", clone, "checkout", "--", "."], check=True)


if __name__ == "__main__":
    main()
