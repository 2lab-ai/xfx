#!/usr/bin/env python3
"""PASTE-GAP round 5's mutants, each on a throwaway `git clone --no-local` of
the code commit, never on the worktree: one textual change, the TUI's lib
tests (`cargo test --locked --lib tui::`) and the pty paste tests
(`cargo test --locked --test tui -- <PTY>`), every failing test named, the
change undone.

    python3 -I -B mutants.py <clone> <out.txt> [<n>,<n>...]

`GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` are unset for every command.
Round 5's rules: the quarantine's sentence leaves the review plane when it
ends (1 never clear; 2 clear only on the quiet's sentence; 3 clear every
status, fx's warning included); each sentence fits two status rows at 20
columns (4, 5, 6: round 4's long sentences back); and two of round 4's,
on round 5's code (7 the hold-change owe; 8 the hold recorded once).
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

MUTANTS = [
    ("item 1: the quarantine's sentence is never taken down", "src/tui/shell.rs",
     "                if screen.clear_status_if(&QUARANTINE_SENTENCES) {\n",
     "                if screen.clear_status_if(&[]) {\n"),
    ("item 1: only the quiet's sentence is taken down", "src/tui/shell.rs",
     "const QUARANTINE_SENTENCES: [&str; 3] = [PASTE_STILL, PASTE_UNTIL_END, PASTE_UNTIL_KEY];\n",
     "const QUARANTINE_SENTENCES: [&str; 3] = [PASTE_STILL, PASTE_STILL, PASTE_STILL];\n"),
    ("item 1: every status is taken down, fx's warning included", "src/tui/approval_screen.rs",
     "            .is_some_and(|status| sentences.contains(&status));\n",
     "            .is_some_and(|_| !sentences.is_empty());\n"),
    ("item 2: the quiet's sentence is round 4's", "src/tui/shell.rs",
     'const PASTE_STILL: &str = "Keys ignored until input stops.";\n',
     'const PASTE_STILL: &str = "Input is still being ignored until it stops for a moment.";\n'),
    ("item 2: the open paste's sentence is round 4's", "src/tui/shell.rs",
     'const PASTE_UNTIL_END: &str = "Paste still open; keys are ignored.";\n',
     'const PASTE_UNTIL_END: &str =\n    "Input is still being ignored; a pause won\'t end it until the paste\'s end arrives.";\n'),
    ("item 2: the open sequence's sentence is round 4's", "src/tui/shell.rs",
     'const PASTE_UNTIL_KEY: &str = "Key sequence open; keys are ignored.";\n',
     'const PASTE_UNTIL_KEY: &str = "Input is still being ignored; a pause won\'t end it until the escape sequence it began is finished or broken.";\n'),
    ("round 4: a hold changed by held bytes alone is never owed", "src/tui/input.rs",
     "        let changed = quarantine.said_hold.is_some_and(|said| said != hold);\n",
     "        let changed = false;\n"),
    ("round 4: the hold is recorded at the first repeat only", "src/tui/input.rs",
     "        quarantine.said_hold = again;\n",
     "        if quarantine.said_hold.is_none() {\n            quarantine.said_hold = again;\n        }\n"),
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


def main():
    clone, out_path = sys.argv[1], sys.argv[2]
    chosen = None
    if len(sys.argv) > 3:
        chosen = {int(n) for n in sys.argv[3].split(",")}
    env = {k: v for k, v in os.environ.items()
           if k not in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE")}
    head = subprocess.run(["git", "-C", clone, "rev-parse", "--short=8", "HEAD"],
                          capture_output=True, text=True, env=env).stdout.strip()
    lines = [f"clone at {head}; per test run timeout {TIMEOUT_S} s"]
    for n, (what, path, old, new) in enumerate(MUTANTS, 1):
        if chosen and n not in chosen:
            continue
        file = os.path.join(clone, path)
        text = open(file, encoding="utf-8", newline="").read()
        if text.count(old) != 1:
            lines.append(f"{n:2}. NOT APPLIED ({text.count(old)} matches): {what}")
            print(lines[-1], flush=True)
            continue
        open(file, "w", encoding="utf-8", newline="").write(text.replace(old, new))
        try:
            verdicts = []
            for cmd in (["cargo", "test", "--locked", "--lib", "tui::"],
                        ["cargo", "test", "--locked", "--test", "tui", "--"] + PTY):
                code, output = run(cmd, clone, env)
                if code is None:
                    verdicts.append("hang")
                elif "error[E" in output or "could not compile" in output:
                    verdicts.append("did not compile")
                else:
                    verdicts.append(failed(output) if code else [])
            names = [v for v in verdicts if isinstance(v, list)]
            killed = [name for v in names for name in v]
            if "did not compile" in verdicts:
                verdict = "DID NOT COMPILE"
            elif killed:
                verdict = f"killed by {len(killed)}: " + ", ".join(killed)
            elif "hang" in verdicts:
                verdict = "killed (hang)"
            else:
                verdict = "SURVIVED"
            lines.append(f"{n:2}. {verdict} -- {what} ({path})")
        finally:
            open(file, "w", encoding="utf-8", newline="").write(text)
        print(lines[-1], flush=True)
    with open(out_path, "w", encoding="utf-8") as out:
        out.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
