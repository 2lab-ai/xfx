#!/usr/bin/env python3
"""PASTE-GAP round 4's mutants, each on a throwaway `git clone --no-local` of
the code commit, never on the worktree: one textual change, the TUI's lib
tests (`cargo test --locked --lib tui::`) and the pty paste tests
(`cargo test --locked --test tui -- <PTY>`), every failing test named, the
change undone.

    python3 -I -B mutants.py <clone> <out.txt> [<n>,<n>...]

`GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` are unset for every command.
Round 4's rules: a hold changed by held bytes alone is owed (1, 2) and owed
once (9); each framing sentence is round 4's (3, 4); `ESC [` holds (5) and a
continuing byte keeps it held (10). Then the lone-Escape release (6), the read
cap (7), and fx's boundary (8), on round 4's code. Mutants 9 and 10 were added
after the first run of 1-8, when mutant 2 proved to test "never owed" rather
than "owed once" and mutant 5 to release `ESC [` rather than `ESC [ 2`.
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
    ("item 2: a hold changed by held bytes alone is never owed", "src/tui/input.rs",
     "        let changed = quarantine.said_hold.is_some_and(|said| said != hold);\n",
     "        let changed = false;\n"),
    ("item 2: the hold behind a repeat is never recorded, so no change is ever seen", "src/tui/input.rs",
     "        quarantine.said_hold = again;\n", ""),
    ("item 1: the marker sentence promises one more key", "src/tui/shell.rs",
     "const PASTE_UNTIL_KEY: &str =\n"
     "    \"Input is still being ignored; a pause won't end it until the escape \\\n"
     "     sequence it began is finished or broken.\";\n",
     "const PASTE_UNTIL_KEY: &str =\n"
     "    \"Input is still being ignored until one more key and a moment's pause.\";\n"),
    ("item 4: the dropped-paste sentence is round 3's", "src/tui/shell.rs",
     "const PASTE_UNTIL_END: &str =\n"
     "    \"Input is still being ignored; a pause won't end it until the paste's end arrives.\";\n",
     "const PASTE_UNTIL_END: &str = \"Input is still being ignored until the paste's end arrives.\";\n"),
    ("`ESC [` (two held bytes) is released by the quiet like a lone ESC", "src/tui/input.rs",
     "        let whole = !self.dropping_paste && self.following <= 1;\n",
     "        let whole = !self.dropping_paste && self.following <= 2;\n"),
    ("round 2: a lone ESC is held like a longer marker", "src/tui/input.rs",
     "        let whole = !self.dropping_paste && self.following <= 1;\n",
     "        let whole = !self.dropping_paste && self.following == 0;\n"),
    ("the read cap says nothing", "src/tui/event_loop.rs",
     "            shell.still_at(Instant::now());\n", ""),
    ("fx's boundary: no quarantine after a paste", "src/tui/input.rs",
     "            self.quarantine = Some(Quarantine {\n"
     "                unsaid: 0,\n"
     "                said_at: (dropped > 0).then_some(now),\n"
     "                said_hold: None,\n"
     "                quiet_since: Some(now),\n"
     "            });\n",
     ""),
    # Added after the first run: the "owed once" half of item 2.
    ("item 2: the hold is recorded at the first repeat only, so a change is owed again and again",
     "src/tui/input.rs",
     "        quarantine.said_hold = again;\n",
     "        if quarantine.said_hold.is_none() {\n            quarantine.said_hold = again;\n        }\n"),
    # Added after the first run: item 1's continuing byte, which mutant 5 did not reach.
    ("item 1: a byte that continues a held marker (`ESC [ 2`) lets the quiet end it",
     "src/tui/input.rs",
     "        let whole = !self.dropping_paste && self.following <= 1;\n",
     "        let whole = !self.dropping_paste && self.following <= 3;\n"),
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
