#!/usr/bin/env python3
"""PASTE-GAP round 7's mutants, each on a throwaway `git clone --no-local`,
never on the worktree: one change (one or more edits), the TUI's lib tests
(`cargo test --locked --lib tui::`) and the pty paste tests
(`cargo test --locked --test tui -- <PTY>`), every failing test named, the
change undone.

    python3 -I -B mutants.py <clone> <out.txt> [<n>,<n>...]

`GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` are unset for every command.
Round 7's rules: only the paste sentences are given up whole (1 every
status given up whole, round 6's rule; 2 the paste sentences cut from their
last row); the plane's line reconciled at the turn's end, not at the two
checks (3); the two readiness refusals condition first (4, 5: round 6's
words back).
"""
import os
import re
import signal
import subprocess
import sys

TIMEOUT_S = 900
PTY = [
    "a_paste_whose_end_marker_shares_a_read_with_enter_is_kept_and_submits_nothing",
    "an_enter_one_read_behind_a_paste_s_end_marker_is_behind_it_too",
    "a_mebibyte_of_hostile_clipboard_written_unpaced_types_nothing_and_sends_nothing",
    "a_mebibyte_of_hostile_clipboard_written_unpaced_answers_no_approval",
]

SHELL = "src/tui/shell.rs"
SCREEN = "src/tui/approval_screen.rs"

MUTANTS = [
    ("item 1: every status given up whole (round 6's rule)", [
        (SCREEN,
         "            if lowest == Part::Status && whole {\n",
         "            if lowest == Part::Status {\n"),
    ]),
    ("item 1: the paste sentences cut from their last row", [
        (SHELL,
         "            Some(screen) => screen.set_whole_status(text.to_string()),\n",
         "            Some(screen) => screen.set_status(text.to_string()),\n"),
    ]),
    ("item 3: reconciled at the end of the two checks, not at the turn's end", [
        (SHELL,
         "        self.consume(events, now);\n        self.plane_names_the_hold();\n    }\n",
         "        self.consume(events, now);\n    }\n"),
        (SHELL,
         "            Delivery::Ignored { again, .. } => self.said_ignored(again),\n        }\n    }\n",
         "            Delivery::Ignored { again, .. } => self.said_ignored(again),\n        }\n"
         "        self.plane_names_the_hold();\n    }\n"),
        (SHELL,
         "        if let Delivery::Ignored { again, .. } = self.decoder.still(now) {\n"
         "            self.said_ignored(again);\n        }\n    }\n",
         "        if let Delivery::Ignored { again, .. } = self.decoder.still(now) {\n"
         "            self.said_ignored(again);\n        }\n"
         "        self.plane_names_the_hold();\n    }\n"),
    ]),
    ("item 2: the not-yet refusal's round 6 words (the key before its condition)", [
        (SHELL,
         '    "the question is not on the screen yet; once you can read it, press it again";\n',
         '    "the question is not on the screen yet - press it again once you can read it";\n'),
    ]),
    ("item 2: the never-whole refusal's round 6 words (the keys before what they do)", [
        (SHELL,
         "     make the window larger, or, to refuse it, press 3 or ctrl+c\";\n",
         "     make the window larger, or press 3 or ctrl+c to refuse\";\n"),
    ]),
]


def run(cmd, cwd, env):
    """`cmd` in its own process group; (exit code or None on a timeout, output)."""
    proc = subprocess.Popen(cmd, cwd=cwd, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, start_new_session=True)
    try:
        out, _ = proc.communicate(timeout=TIMEOUT_S)
        return proc.returncode, out.decode("utf-8", "replace")
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        proc.communicate()
        return None, ""


def failed(output):
    return sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED$", output, re.M)))


def passed(output):
    return sorted(set(re.findall(r"^test (\S+) \.\.\. ok$", output, re.M)))


def main():
    args = sys.argv[1:]
    clone, out_path = args[0], args[1]
    chosen = None
    if len(args) > 2:
        chosen = {int(n) for n in args[2].split(",")}
    env = {k: v for k, v in os.environ.items()
           if k not in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE")}
    head = subprocess.run(["git", "-C", clone, "rev-parse", "--short=8", "HEAD"],
                          capture_output=True, text=True, env=env).stdout.strip()
    lines = [f"clone at {head}; per test run timeout {TIMEOUT_S} s"]
    table = dict(enumerate(MUTANTS, 1))
    commands = [["cargo", "test", "--locked", "--lib", "tui::"],
                ["cargo", "test", "--locked", "--test", "tui", "--"] + PTY]
    for n, (what, edits) in table.items():
        if chosen and n not in chosen:
            continue
        saved = {}
        applied = True
        for path, old, new in edits:
            file = os.path.join(clone, path)
            text = open(file, encoding="utf-8", newline="").read()
            saved.setdefault(file, text)
            if text.count(old) != 1:
                lines.append(f"{n:2}. NOT APPLIED ({text.count(old)} matches in {path}): {what}")
                applied = False
                break
            open(file, "w", encoding="utf-8", newline="").write(text.replace(old, new))
        try:
            if not applied:
                print(lines[-1], flush=True)
                continue
            verdicts = []
            ran = []
            for cmd in commands:
                code, output = run(cmd, clone, env)
                if code is None:
                    verdicts.append("hang")
                elif "error[E" in output or "could not compile" in output:
                    verdicts.append("did not compile")
                else:
                    verdicts.append(failed(output) if code else [])
                    ran.extend(passed(output))
            names = [v for v in verdicts if isinstance(v, list)]
            killed = [name for v in names for name in v]
            if "did not compile" in verdicts:
                verdict = "DID NOT COMPILE"
            elif killed:
                verdict = f"killed by {len(killed)}: " + ", ".join(killed)
            elif "hang" in verdicts:
                verdict = "killed (hang)"
            else:
                verdict = f"SURVIVED ({len(ran)} tests passed)"
            lines.append(f"{n:2}. {verdict} -- {what}")
        finally:
            for file, text in saved.items():
                open(file, "w", encoding="utf-8", newline="").write(text)
        print(lines[-1], flush=True)
    with open(out_path, "w", encoding="utf-8") as out:
        out.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
