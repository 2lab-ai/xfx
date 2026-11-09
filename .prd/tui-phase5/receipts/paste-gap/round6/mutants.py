#!/usr/bin/env python3
"""PASTE-GAP round 6's mutants, each on a throwaway `git clone --no-local`,
never on the worktree: one change (one or more edits), the TUI's lib tests
(`cargo test --locked --lib tui::`) and the pty paste tests
(`cargo test --locked --test tui -- <PTY>`), every failing test named, the
change undone.

    python3 -I -B mutants.py <clone> <out.txt> [<n>,<n>...] [--round5]

`GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` are unset for every command.
Round 6's rules: the plane's quarantine line follows the decoder at the end
of every turn (1 clear only on release, round 5's rule; 2 the reconcile on
the drained path only, so the read cap's turn skips it; 3 the hold compared
without the quarantine it was said in; 4 no repaint; 5 fx's warning taken
down too; 10 no reconcile at all); the plane's status rows (6 two rows; 7
given up from the last row); and the flapping test's observer (8 the first
repeat half a repeat after fx's warning; 9 fx's warning said again behind a
repeat, in the same turn).

With `--round5` the clone is at round 5's code commit and only mutants 8 and
9 run, in round 5's code, against round 5's flapping test alone: whether
round 5's observer saw them.
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
INPUT = "src/tui/input.rs"
SCREEN = "src/tui/approval_screen.rs"

FIRST_REPEAT_EARLY = (
    INPUT,
    "        quarantine.said_at = Some(now);\n        quarantine.said_hold = again;\n",
    "        quarantine.said_at = Some(if again.is_none() {\n"
    "            now.checked_sub(Self::PASTE_REPEAT / 2).unwrap_or(now)\n"
    "        } else {\n"
    "            now\n"
    "        });\n        quarantine.said_hold = again;\n",
)

MUTANTS = [
    ("item 1: cleared only on release (round 5's rule)", [
        (SHELL,
         "        let named = self.decoder.named_hold().map(quarantine_sentence);\n",
         "        if self.decoder.quarantined() {\n            return;\n        }\n"
         "        let named: Option<&str> = None;\n"),
    ]),
    ("item 1: reconciled on the drained path only -- the read cap's turn skips it", [
        (SHELL,
         "        self.consume(events, now);\n        self.plane_names_the_hold();\n    }\n",
         "        self.consume(events, now);\n    }\n"),
        (SHELL,
         "            Delivery::Ignored { again, .. } => self.said_ignored(again),\n        }\n    }\n",
         "            Delivery::Ignored { again, .. } => self.said_ignored(again),\n        }\n"
         "        self.plane_names_the_hold();\n    }\n"),
    ]),
    ("item 1: the hold compared without the quarantine it was said in", [
        (INPUT,
         "        let said = self.quarantine.as_ref()?.said_hold?;\n"
         "        (said == self.hold()).then_some(said)\n",
         "        self.quarantine.as_ref().map(|_| self.hold())\n"),
    ]),
    ("item 3(2): the plane is not repainted when its line is taken down", [
        (SHELL,
         "        if screen.clear_status_if(&stale) {\n            self.render.request(Reason::Modal);\n        }\n",
         "        let _ = screen.clear_status_if(&stale);\n"),
    ]),
    ("item 1: fx's warning is taken down too", [
        (SHELL,
         "        let stale: Vec<&str> = QUARANTINE_SENTENCES\n",
         "        let stale: Vec<&str> = [PASTE_STILL, PASTE_UNTIL_END, PASTE_UNTIL_KEY, PASTE_SUFFIX]\n"),
    ]),
    ("item 3(4): the plane's status has two rows again", [
        (SCREEN, "const STATUS_ROWS: usize = 3;\n", "const STATUS_ROWS: usize = 2;\n"),
    ]),
    ("item 3(3): a status too tall is given up from its last row again", [
        (SCREEN,
         "            if lowest == Part::Status {\n"
         "                parts.retain(|(part, _)| *part != Part::Status);\n"
         "                continue;\n            }\n",
         ""),
    ]),
    ("item 2: the first repeat half a repeat after fx's warning", [FIRST_REPEAT_EARLY]),
    ("item 2: fx's warning said again behind a repeat, in the same turn", [
        (SHELL,
         "            Some(hold) => self.said_after_paste(quarantine_sentence(hold)),\n",
         "            Some(hold) => {\n"
         "                self.said_after_paste(quarantine_sentence(hold));\n"
         "                self.ignored_behind_paste();\n            }\n"),
    ]),
    ("item 1: no reconcile at all", [
        (SHELL,
         "        self.consume(events, now);\n        self.plane_names_the_hold();\n    }\n",
         "        self.consume(events, now);\n    }\n"),
    ]),
]

# Mutants 8 and 9 in round 5's code (f9328d4e), for round 5's flapping test.
ROUND5 = {
    8: ("item 2 in round 5's code: the first repeat half a repeat after fx's warning",
        [FIRST_REPEAT_EARLY]),
    9: ("item 2 in round 5's code: fx's warning said again behind a repeat, in the same turn", [
        (SHELL,
         "            Some(Hold::Quiet) => self.said_after_paste(PASTE_STILL),\n"
         "            Some(Hold::Paste) => self.said_after_paste(PASTE_UNTIL_END),\n"
         "            Some(Hold::Marker) => self.said_after_paste(PASTE_UNTIL_KEY),\n",
         "            Some(Hold::Quiet) => {\n"
         "                self.said_after_paste(PASTE_STILL);\n"
         "                self.ignored_behind_paste();\n            }\n"
         "            Some(Hold::Paste) => {\n"
         "                self.said_after_paste(PASTE_UNTIL_END);\n"
         "                self.ignored_behind_paste();\n            }\n"
         "            Some(Hold::Marker) => {\n"
         "                self.said_after_paste(PASTE_UNTIL_KEY);\n"
         "                self.ignored_behind_paste();\n            }\n"),
    ]),
}
FLAPPING = "a_hold_that_flaps_is_said_at_most_once_a_second_and_its_last_state_is_said"


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
    args = [a for a in sys.argv[1:] if a != "--round5"]
    round5 = "--round5" in sys.argv
    clone, out_path = args[0], args[1]
    chosen = None
    if len(args) > 2:
        chosen = {int(n) for n in args[2].split(",")}
    env = {k: v for k, v in os.environ.items()
           if k not in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE")}
    head = subprocess.run(["git", "-C", clone, "rev-parse", "--short=8", "HEAD"],
                          capture_output=True, text=True, env=env).stdout.strip()
    lines = [f"clone at {head}; per test run timeout {TIMEOUT_S} s"]
    table = ROUND5 if round5 else dict(enumerate(MUTANTS, 1))
    commands = ([["cargo", "test", "--locked", "--lib", "--", FLAPPING]] if round5 else
                [["cargo", "test", "--locked", "--lib", "tui::"],
                 ["cargo", "test", "--locked", "--test", "tui", "--"] + PTY])
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
