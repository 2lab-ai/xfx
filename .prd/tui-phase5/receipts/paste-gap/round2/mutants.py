#!/usr/bin/env python3
"""PASTE-GAP round 2's mutants, each on a throwaway `git clone --no-local` of
the code commit, never on the worktree: one textual change, the TUI's lib
tests (`cargo test --locked --lib tui::`) and the pty paste tests
(`cargo test --locked --test tui -- <PTY>`, the two unpaced mebibyte cases
among them), every failing test named, the change undone.

    python3 -I -B mutants.py <clone> <out.txt> [<n>,<n>...]

A mutant is killed when at least one test fails, and killed (hang) when a test
run passes TIMEOUT_S, its process group killed then. `GIT_DIR`,
`GIT_INDEX_FILE` and `GIT_WORK_TREE` are unset for every command. Each
reverts or half-reverts one rule of round 2: a start marker in the quarantine
begins a paste (1, 11); the quarantine does not end with a marker half read
(2) or a second paste being dropped (3); behind the end marker the markers
are followed (4), to the dropped paste's end marker (5); what the quarantine dropped
before a start marker is still said (6); the quiet is not restarted by the
empty checks before its deadline (7); and input that keeps arriving is said
again once a second, in xfx's words (8, 9, 10). Mutant 12 is fx's boundary,
no quarantine at all, so that the unpaced pty cases can be seen to detect it.
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
    ("round 1: a start marker in the quarantine is dropped with the rest", "src/tui/input.rs",
     'const PASTE_START: &[u8] = b"\\x1b[200~";\n',
     'const PASTE_START: &[u8] = b"\\x00\\x00\\x00\\x00\\x00\\x00";\n'),
    ("the quarantine ends with a marker half read", "src/tui/input.rs",
     "        let whole = !self.dropping_paste && self.following == 0;\n",
     "        let whole = !self.dropping_paste;\n"),
    ("the quarantine ends with a second paste still being dropped", "src/tui/input.rs",
     "        let whole = !self.dropping_paste && self.following == 0;\n",
     "        let whole = self.following == 0;\n"),
    ("behind the end marker the markers are not followed", "src/tui/input.rs",
     "            // Counted, and followed: a second paste begun here is being\n"
     "            // dropped until its own end marker, whenever that comes.\n"
     "            self.follow(byte);\n",
     ""),
    ("a paste being dropped is never closed: only start markers are followed", "src/tui/input.rs",
     "        let marker = if self.dropping_paste {\n            PASTE_END\n        } else {\n            PASTE_START\n        };\n",
     "        let marker = PASTE_START;\n"),
    ("what the quarantine dropped before a start marker is never said", "src/tui/input.rs",
     "                if quarantine.said_at.is_none() {\n                    self.owed = quarantine.unsaid;\n                }\n",
     ""),
    ("every empty check restarts the quiet", "src/tui/input.rs",
     "            Some(_) => {}\n            // Something was read",
     "            Some(_) => quarantine.quiet_since = Some(now),\n            // Something was read"),
    ("input that keeps arriving is never said again", "src/tui/input.rs",
     "            Some(at) if now.saturating_duration_since(at) >= Self::PASTE_REPEAT => true,\n",
     ""),
    ("input that keeps arriving is said again at every check", "src/tui/input.rs",
     "            Some(at) if now.saturating_duration_since(at) >= Self::PASTE_REPEAT => true,\n",
     "            Some(at) if now.saturating_duration_since(at) >= Duration::ZERO => true,\n"),
    ("the repeat is said in fx's words", "src/tui/shell.rs",
     "            Delivery::Ignored { again: true, .. } => self.said_after_paste(PASTE_STILL),\n",
     "            Delivery::Ignored { again: true, .. } => self.ignored_behind_paste(),\n"),
    ("a paste begun in the quarantine is not a paste: its bytes are decoded", "src/tui/input.rs",
     "                self.dropping_paste = false;\n                self.pasting = true;\n",
     "                self.dropping_paste = false;\n"),
    ("fx's boundary: no quarantine after a paste", "src/tui/input.rs",
     "            self.quarantine = Some(Quarantine {\n"
     "                unsaid: 0,\n"
     "                said_at: (dropped > 0).then_some(now),\n"
     "                quiet_since: Some(now),\n"
     "            });\n",
     ""),
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
